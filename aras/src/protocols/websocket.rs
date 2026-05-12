use std::collections::VecDeque;
use std::fmt::Display;
use std::result::Result as StdResult;
use std::string::FromUtf8Error;
use std::time::Duration;

use asgispec::events::{WebsocketAcceptEvent, WebsocketCloseEvent, WebsocketSendEvent};
use asgispec::prelude::*;
use bytes::Bytes;
use derive_more::derive::Constructor;
use fastwebsockets::upgrade::UpgradeFut;
use fastwebsockets::{
    upgrade, CloseCode, FragmentCollector, Frame, OpCode, Payload, WebSocketError,
};
use http::StatusCode;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Body;
use hyper::Request;
use tracing::{error, info};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::communication::{ApplicationResult, CommunicationResult, ReceiveFromASGIApp, SendToASGIApp};
use crate::types::Response;
use crate::{ArasError, ArasResult};

#[derive(Constructor)]
pub(crate) struct WebsocketHandler {
    timeout: Duration,
    max_frame_size: usize,
    client: String,
}

impl WebsocketHandler {
    pub async fn handle<B>(
        self,
        mut send_to_app: impl SendToASGIApp + 'static,
        mut receive_from_app: impl ReceiveFromASGIApp + 'static,
        mut request: Request<B>,
    ) -> ArasResult<Response>
    where
        B: Body + Send + 'static,
        B::Error: Display,
    {
        let app_response = self.accept(&mut send_to_app, &mut receive_from_app).await?;
        if !app_response.accepted() {
            return Ok(app_response.into());
        };

        let (upgrade_response, upgrade_future) = upgrade::upgrade(&mut request)?;
        tokio::task::spawn(self.run_protocol(upgrade_future, send_to_app, receive_from_app));

        merge_responses(app_response.into(), upgrade_response)
    }

    async fn accept(
        &self,
        send_to: &mut impl SendToASGIApp,
        receive_from: &mut impl ReceiveFromASGIApp,
    ) -> ArasResult<ConnectResponse> {
        send_to
            .send(ASGIReceiveEvent::new_websocket_connect())
            .await?;
        tokio::time::timeout(self.timeout, receive_from.receive())
            .await??
            .try_into()
    }

    async fn run_protocol(
        self,
        upgrade_future: UpgradeFut,
        send_to_app: impl SendToASGIApp + 'static,
        receive_from_app: impl ReceiveFromASGIApp + 'static,
    ) {
        let ws = match upgrade_future.await {
            // ASGI requires unfragmented messages to the application. So just use FragmentCollector.
            Ok(ws) => FragmentCollector::new(ws),
            Err(e) => {
                error!("Websocket upgrade failed: {}", e);
                return;
            }
        };

        let mut state = State::Starting;
        let mut context = Context::new(self.max_frame_size, send_to_app, receive_from_app, ws);
        info!("Connecting websocket client: {}", self.client);

        while !matches!(state, State::Closed) {
            let event = Event::next(&mut context).await;
            state = state.on_event(event, &mut context).await;
        }

        info!("Websocket connection closed for client: {}", self.client);
    }
}

enum ConnectResponse {
    Accept(Response),
    Close(Response),
}

impl ConnectResponse {
    pub fn accepted(&self) -> bool {
        matches!(self, ConnectResponse::Accept(_))
    }
}

impl From<ConnectResponse> for Response {
    fn from(value: ConnectResponse) -> Self {
        match value {
            ConnectResponse::Accept(resp) => resp,
            ConnectResponse::Close(resp) => resp,
        }
    }
}

impl TryFrom<ASGISendEvent> for ConnectResponse {
    type Error = ArasError;

    fn try_from(value: ASGISendEvent) -> Result<Self, Self::Error> {
        match value {
            ASGISendEvent::WebsocketAccept(msg) => ConnectResponse::try_from(msg),
            ASGISendEvent::WebsocketClose(msg) => ConnectResponse::try_from(msg),
            msg => Err(ArasError::unexpected_asgi_message(&format!("{msg:?}"))),
        }
    }
}

impl TryFrom<WebsocketAcceptEvent> for ConnectResponse {
    type Error = ArasError;

    fn try_from(value: WebsocketAcceptEvent) -> Result<Self, Self::Error> {
        let mut builder = http::Response::builder();
        let body = Full::new(Vec::<u8>::new().into())
            .map_err(|never| match never {})
            .boxed();
        builder = builder.status(StatusCode::SWITCHING_PROTOCOLS);
        if let Some(subprotocol) = value.subprotocol {
            builder = builder.header(hyper::header::SEC_WEBSOCKET_PROTOCOL, subprotocol);
        }
        for (bytes_key, bytes_value) in value.headers.into_iter() {
            builder = builder.header(bytes_key.to_vec(), bytes_value.to_vec());
        }
        Ok(Self::Accept(builder.body(body)?))
    }
}

impl TryFrom<WebsocketCloseEvent> for ConnectResponse {
    type Error = ArasError;

    fn try_from(value: WebsocketCloseEvent) -> Result<Self, Self::Error> {
        let body = Full::new(value.reason.into())
            .map_err(|never| match never {})
            .boxed();
        let mut builder = http::Response::builder();
        builder = builder.status(StatusCode::FORBIDDEN);
        Ok(Self::Close(builder.body(body)?))
    }
}

struct Context<S, R, W> {
    frame_builder: FrameBuilder,
    send_to_app: S,
    receive_from_app: R,
    websocket: FragmentCollector<W>,
}

impl<S, R, W> Context<S, R, W> {
    pub fn new(
        max_frame_size: usize,
        send_to_app: S,
        receive_from_app: R,
        websocket: FragmentCollector<W>,
    ) -> Self {
        let frame_builder = FrameBuilder::new(max_frame_size);
        Self {
            frame_builder,
            send_to_app,
            receive_from_app,
            websocket,
        }
    }
}

trait Websocket: AsyncRead + AsyncWrite + Unpin {}
impl<T: AsyncRead + AsyncWrite + Unpin> Websocket for T {}

enum State {
    Starting,
    Running,
    Closed,
}

impl State {
    pub async fn on_event<W, S, R>(&self, event: Event, ctx: &mut Context<S, R, W>) -> Self
    where
        W: Websocket,
        S: SendToASGIApp,
    {
        if let Self::Closed = self {
            return Self::Closed;
        }

        match event {
            Event::FrameReceived(frame) => self.on_frame_received(frame, ctx).await,
            Event::ASGIEventReceived(asgi) => self.on_asgi_received(asgi, ctx).await,
            Event::ErrorOccurred(error) => self.on_error(error, ctx).await,
            Event::ConnectionClosed => self.close(CloseCode::Normal.into(), "Connection closed".into(), ctx).await,
        }
    }

    async fn on_frame_received<W, S, R>(
        &self,
        frame: ASGIReceiveEvent,
        ctx: &mut Context<S, R, W>,
    ) -> Self
    where
        W: Websocket,
        S: SendToASGIApp,
    {
        if let ASGIReceiveEvent::WebsocketDisconnect(msg) = frame {
            return self.close(msg.code, msg.reason, ctx).await;
        };

        match ctx.send_to_app.send(frame).await {
            Ok(_) => Self::Running,
            Err(e) => self.on_error(e.into(), ctx).await,
        }
    }

    async fn on_asgi_received<W, S, R>(
        &self,
        asgi: ASGISendEvent,
        ctx: &mut Context<S, R, W>,
    ) -> Self
    where
        W: Websocket,
        S: SendToASGIApp,
    {
        let ws_send = match asgi {
            ASGISendEvent::WebsocketSend(msg) => msg,
            ASGISendEvent::WebsocketClose(msg) => {
                return self.close(msg.code, msg.reason, ctx).await
            }
            _ => {
                return self
                    .on_error(ArasError::unexpected_asgi_message(&format!("{asgi:?}")), ctx)
                    .await
            }
        };

        let mut frames = ctx.frame_builder.build(ws_send);

        while let Some(frame) = frames.pop_front() {
            if let Err(e) = ctx.websocket.write_frame(frame).await {
                return self.on_error(e.into(), ctx).await;
            }
        }

        Self::Running
    }

    async fn on_error<W, S, R>(&self, error: ArasError, ctx: &mut Context<S, R, W>) -> Self
    where
        W: Websocket,
        S: SendToASGIApp,
    {
        if let ArasError::WsUtf8Error(e) = &error {
            error!("Websocket received invalid UTF-8 data: {}", e);
            return self
                .close(CloseCode::Invalid.into(), "Invalid UTF-8 data received".into(), ctx)
                .await;
        }

        if let ArasError::WsInvalidCloseCode(code) = &error {
            error!("Websocket received invalid close code: {}", code);
            return self
                .close(CloseCode::Protocol.into(), "Invalid close code".into(), ctx)
                .await;
        }

        error!("Error during websocket connection: {}", error);
        self.close(CloseCode::Error.into(), "Internal server error".into(), ctx)
            .await
    }

    async fn close<W, S, R>(&self, code: u16, msg: String, ctx: &mut Context<S, R, W>) -> Self
    where
        W: Websocket,
        S: SendToASGIApp,
    {
        info!("Closing websocket with code {code}");
        let frame = Frame::close(code, msg.as_bytes());
        let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(code, msg);
        let _ = ctx.websocket.write_frame(frame).await;
        let _ = ctx.send_to_app.send(asgi_event).await;
        Self::Closed
    }
}

enum Event {
    FrameReceived(ASGIReceiveEvent),
    ASGIEventReceived(ASGISendEvent),
    ConnectionClosed,
    ErrorOccurred(ArasError),
}

impl Event {
    pub async fn next<S, R, W>(ctx: &mut Context<S, R, W>) -> Self
    where
        W: Websocket,
        R: ReceiveFromASGIApp,
    {
        let ws = &mut ctx.websocket;
        let app = &mut ctx.receive_from_app;

        tokio::select! {
            out = ws.read_frame() => Event::from(out),
            out = app.receive() => Event::from(out),
        }
    }
}

impl From<StdResult<Frame<'_>, WebSocketError>> for Event {
    fn from(value: StdResult<Frame<'_>, WebSocketError>) -> Self {
        match value {
            Ok(frame) => Self::from(frame),
            Err(WebSocketError::ConnectionClosed) => Self::ConnectionClosed,
            Err(e) => Self::ErrorOccurred(e.into()),
        }
    }
}

impl From<Frame<'_>> for Event {
    fn from(value: Frame<'_>) -> Self {
        let bytes = match value.payload {
            Payload::Bytes(b) => Some(Bytes::from(b)),
            Payload::Owned(b) => Some(Bytes::from(b)),
            Payload::Borrowed(b) => Some(Bytes::copy_from_slice(b)),
            Payload::BorrowedMut(b) => Some(Bytes::copy_from_slice(b)),
        };

        // RFC 6455 §7.4.2: valid codes are 1000-1003, 1007-1011, 3000-4999
        fn is_valid_close_code(code: u16) -> bool {
            matches!(code, 1000..=1003 | 1007..=1011 | 3000..=4999)
        }

        match value.opcode {
            OpCode::Text => {
                let data = bytes.unwrap_or(Bytes::new());
                Self::from(String::from_utf8(data.to_vec()))
            }
            OpCode::Binary => {
                let asgi_event = ASGIReceiveEvent::new_websocket_receive(bytes, None);
                Self::FrameReceived(asgi_event)
            }
            OpCode::Close => {
                let data = bytes.unwrap_or(Bytes::new());
                let (code, reason) = if data.len() >= 2 {
                    let code = u16::from_be_bytes([data[0], data[1]]);
                    let reason = String::from_utf8_lossy(&data[2..]).into_owned();
                    (code, reason)
                } else {
                    (CloseCode::Normal.into(), String::new())
                };
                if !is_valid_close_code(code) {
                    return Self::ErrorOccurred(ArasError::WsInvalidCloseCode(code));
                }
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(code, reason);
                Self::FrameReceived(asgi_event)
            }
            op_code => {
                Self::ErrorOccurred(ArasError::custom(format!("unexpected opcode: {op_code:?}")))
            }
        }
    }
}

impl From<Result<String, FromUtf8Error>> for Event {
    fn from(value: Result<String, FromUtf8Error>) -> Self {
        match value {
            Ok(text) => Self::FrameReceived(ASGIReceiveEvent::new_websocket_receive(None, Some(text.into()))),
            Err(e) => Self::ErrorOccurred(e.into()),
        }
    }
}   

impl From<CommunicationResult<ASGISendEvent>> for Event {
    fn from(value: CommunicationResult<ASGISendEvent>) -> Self {
        match value {
            Ok(msg) => Self::from(msg),
            Err(ApplicationResult::Completed) => Self::ConnectionClosed,
            Err(e) => Self::ErrorOccurred(e.into()),
        }
    }
}

impl From<ASGISendEvent> for Event {
    fn from(value: ASGISendEvent) -> Self {
        match value {
            ASGISendEvent::WebsocketSend(msg) => {
                Self::ASGIEventReceived(ASGISendEvent::WebsocketSend(msg))
            }
            ASGISendEvent::WebsocketClose(msg) => {
                Self::ASGIEventReceived(ASGISendEvent::WebsocketClose(msg))
            }
            event => Self::ErrorOccurred(ArasError::unexpected_asgi_message(&format!("{event:?}")).into()),
        }
    }
}

#[derive(Constructor)]
struct FrameBuilder {
    max_frame_size: usize,
}

impl FrameBuilder {
    fn build(&self, asgi_event: WebsocketSendEvent) -> VecDeque<Frame<'static>> {
        let (chunks, op_code) = match (asgi_event.text, asgi_event.bytes) {
            (Some(text), _) => (self.split_string(text), OpCode::Text),
            (_, Some(bytes)) => (self.split_bytes(bytes), OpCode::Binary),
            _ => {
                return VecDeque::from([Frame::new(
                    true,
                    OpCode::Binary,
                    None,
                    Payload::Owned(vec![]),
                )])
            }
        };

        let total = chunks.len();
        chunks
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| {
                let fin = i == total - 1;
                let opcode = if i == 0 { op_code } else { OpCode::Continuation };
                Frame::new(fin, opcode, None, Payload::Owned(chunk.to_vec()))
            })
            .collect()
    }

    fn split_bytes(&self, data: Bytes) -> Vec<Bytes> {
        if data.len() <= self.max_frame_size {
            return vec![data];
        }

        let mut chunks = Vec::new();
        let mut start = 0;
        while start < data.len() {
            let end = (start + self.max_frame_size).min(data.len());
            chunks.push(data.slice(start..end));
            start = end;
        }
        chunks
    }

    fn split_string(&self, data: String) -> Vec<Bytes> {
        self.split_bytes(Bytes::from(data))
    }
}

fn merge_responses(
    app_response: Response,
    upgrade_response: http::Response<Empty<Bytes>>,
) -> ArasResult<Response> {
    let mut merged_response = http::Response::builder().status(upgrade_response.status());
    for (k, v) in upgrade_response.headers() {
        merged_response = merged_response.header(k, v);
    }
    for (k, v) in app_response.headers() {
        merged_response = merged_response.header(k, v);
    }
    let body = app_response.into_body();
    Ok(merged_response.body(body)?)
}

#[cfg(test)]
mod test_utils {
    use crate::mocks::stream::MockWebsocketStream;
    use fastwebsockets::{FragmentCollector, Role, WebSocket};

    pub fn build_websocket_client(
        messages: Option<Vec<&str>>,
    ) -> (MockWebsocketStream, FragmentCollector<MockWebsocketStream>) {
        let stream;
        if messages.is_some() {
            stream = MockWebsocketStream::new(messages.unwrap());
        } else {
            stream = MockWebsocketStream::empty();
        }
        let ws = WebSocket::after_handshake(stream.clone(), Role::Client);
        let arc_ws = FragmentCollector::new(ws);
        (stream, arc_ws)
    }
}

#[cfg(test)]
mod test_state_transitions {
    use asgispec::prelude::*;
    use fastwebsockets::OpCode;

    use super::{test_utils::build_websocket_client, Context, Event, State};
    use crate::mocks::communication::{DeterministicReceiveFromApp, SendToAppCollector};
    use crate::mocks::stream::FrameExt;

    #[tokio::test]
    async fn test_running_asgi_send_received() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Running;
        let event = Event::ASGIEventReceived(ASGISendEvent::new_websocket_send(
            None,
            Some("hello".into()),
        ));
        let mut context = Context::new(1024, send_to, receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Running));
        let frames = stream.written_frames().await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(frames[0].message().unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_running_frame_data_received() {
        let (_, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Running;
        let event = Event::FrameReceived(ASGIReceiveEvent::new_websocket_receive(
            None,
            Some("frame".into()),
        ));
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Running));
        let send_to_app = send_to.get_messages().await;
        assert_eq!(send_to_app.len(), 1);
        assert_eq!(
            send_to_app[0],
            ASGIReceiveEvent::new_websocket_receive(None, Some("frame".into()))
        );
    }

    #[tokio::test]
    async fn test_running_error_occurred() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Running;
        let event = Event::ErrorOccurred(crate::ArasError::Custom("whoops".into()));
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Closed));
        let frames = stream.written_frames().await;
        let send_to_app = send_to.get_messages().await;
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_close_frame());
        assert_eq!(frames[0].close_code().unwrap(), 1011);
        assert!(frames[0].message().unwrap().contains("Internal server error"));
        assert_eq!(send_to_app.len(), 1);
        assert!(matches!(
            send_to_app[0],
            ASGIReceiveEvent::WebsocketDisconnect(_)
        ));
    }

    #[tokio::test]
    async fn test_running_close_frame_received() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Running;
        let event = Event::FrameReceived(ASGIReceiveEvent::new_websocket_disconnect(
            1000,
            "goodbye".into(),
        ));
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Closed));
        let frames = stream.written_frames().await;
        let send_to_app = send_to.get_messages().await;
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_close_frame());
        assert_eq!(frames[0].close_code().unwrap(), 1000);
        assert_eq!(frames[0].message().unwrap(), "goodbye");
        assert_eq!(send_to_app.len(), 1);
        assert!(matches!(
            send_to_app[0],
            ASGIReceiveEvent::WebsocketDisconnect(_)
        ));
    }

    #[tokio::test]
    async fn test_running_close_asgi_received() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Running;
        let event =
            Event::ASGIEventReceived(ASGISendEvent::new_websocket_close(1000, "goodbye".into()));
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Closed));
        let frames = stream.written_frames().await;
        let send_to_app = send_to.get_messages().await;
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_close_frame());
        assert_eq!(frames[0].close_code().unwrap(), 1000);
        assert_eq!(frames[0].message().unwrap(), "goodbye");
        assert_eq!(send_to_app.len(), 1);
        assert!(matches!(
            send_to_app[0],
            ASGIReceiveEvent::WebsocketDisconnect(_)
        ));
    }

    #[tokio::test]
    async fn test_closed_any_event() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Closed;
        let event =
            Event::ASGIEventReceived(ASGISendEvent::new_websocket_close(1000, "goodbye".into()));
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Closed));
        let frames = stream.written_frames().await;
        let send_to_app = send_to.get_messages().await;
        assert!(frames.is_empty());
        assert!(send_to_app.is_empty());
    }

    #[tokio::test]
    async fn test_running_connection_closed() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Running;
        let event = Event::ConnectionClosed;
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Closed));
        let frames = stream.written_frames().await;
        let send_to_app = send_to.get_messages().await;
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_close_frame());
        assert_eq!(frames[0].close_code().unwrap(), 1000);
        assert_eq!(frames[0].message().unwrap(), "Connection closed");
        assert!(matches!(
            send_to_app[0],
            ASGIReceiveEvent::WebsocketDisconnect(_)
        ));
    }

    #[tokio::test]
    async fn test_starting_connection_closed() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let state = State::Starting;
        let event = Event::ConnectionClosed;
        let mut context = Context::new(1024, send_to.clone(), receive_from, ws);

        let new_state = state.on_event(event, &mut context).await;

        assert!(matches!(new_state, State::Closed));
        let frames = stream.written_frames().await;
        let send_to_app = send_to.get_messages().await;
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_close_frame());
        assert_eq!(frames[0].close_code().unwrap(), 1000);
        assert_eq!(frames[0].message().unwrap(), "Connection closed");
        assert!(matches!(
            send_to_app[0],
            ASGIReceiveEvent::WebsocketDisconnect(_)
        ));
    }
}

#[cfg(test)]
mod test_handler {
    use super::{test_utils::build_websocket_client, Context, Event, State, WebsocketHandler};

    use std::time::Duration;
    use std::vec;

    use asgispec::prelude::*;
    use bytes::Bytes;
    use fastwebsockets::OpCode;
    use http::Request;
    use tokio::io::{AsyncRead, AsyncWrite};

    use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
    use crate::mocks::communication::{DeterministicReceiveFromApp, SendToAppCollector};
    use crate::mocks::stream::FrameExt;

    fn build_websocket_request() -> Request<String> {
        Request::builder()
            .header("Connection", "Upgrade")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("sec-websocket-version", "13")
            .body("".to_string())
            .expect("Failed to build request")
    }

    async fn run_ws_until_complete<S, R, W>(mut context: Context<S, R, W>)
    where
        W: AsyncRead + AsyncWrite + Unpin,
        S: SendToASGIApp,
        R: ReceiveFromASGIApp,
    {
        let mut state = State::Starting;

        while !matches!(state, State::Closed) {
            let event = Event::next(&mut context).await;
            state = state.on_event(event, &mut context).await;
        }
    }

    #[tokio::test]
    async fn test_asgi_messages_are_send_to_client() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_websocket_send(
                None,
                Some("hello client".into()),
            )),
            Ok(ASGISendEvent::new_websocket_send(
                None,
                Some("im the server".into()),
            )),
            Ok(ASGISendEvent::new_websocket_close(1000, "goodbye".into())),
        ]);

        let ctx = Context::new(1024, send_to.clone(), receive_from, ws);
        run_ws_until_complete(ctx).await;

        let frames = stream.written_frames().await;
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(frames[0].message().unwrap(), "hello client");
        assert_eq!(frames[1].opcode, OpCode::Text);
        assert_eq!(frames[1].message().unwrap(), "im the server");
        assert!(frames[2].is_close_frame());
        assert_eq!(frames[2].close_code().unwrap(), 1000);
        assert_eq!(frames[2].message().unwrap(), "goodbye");

        let send_to_app = send_to.get_messages().await;
        assert_eq!(send_to_app.len(), 1);
        assert!(matches!(
            send_to_app[0],
            ASGIReceiveEvent::WebsocketDisconnect(_)
        ));
    }

    #[tokio::test]
    async fn test_frames_split_when_exceeding_max_message_size() {
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_websocket_send(
                None,
                Some("I would very much like to be split up".into()),
            )),
            Ok(ASGISendEvent::new_websocket_close(1000, "goodbye".into())),
        ]);

        let ctx = Context::new(10, send_to.clone(), receive_from, ws);
        run_ws_until_complete(ctx).await;

        let frames = stream.written_frames().await;
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(frames[0].message().unwrap(), "I would ve");
        assert_eq!(frames[3].message().unwrap(), "plit up");
        assert!(frames[4].is_close_frame());
    }

    #[tokio::test]
    async fn test_client_messages_are_send_to_asgi_app() {
        let (_, ws) = build_websocket_client(Some(vec!["hello server", "im the client"]));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let ctx = Context::new(1024, send_to.clone(), receive_from, ws);
        run_ws_until_complete(ctx).await;

        let send_to_app = send_to.get_messages().await;
        assert!(send_to_app.len() == 3);
        assert!(
            send_to_app[0]
                == ASGIReceiveEvent::new_websocket_receive(None, Some("hello server".into()))
        );
        assert!(
            send_to_app[1]
                == ASGIReceiveEvent::new_websocket_receive(None, Some("im the client".into()))
        );
    }

    #[tokio::test]
    async fn test_accept_websocket_connection() {
        let handler = WebsocketHandler::new(Duration::from_secs(5), 1000, "client".into());
        let request = build_websocket_request();

        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![Ok(
            ASGISendEvent::new_websocket_accept(None, vec![]),
        )]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_ok());

        let response = response.unwrap();

        assert!(response.status() == http::StatusCode::SWITCHING_PROTOCOLS);
        assert!(response.headers().get("sec-websocket-accept").is_some());
        assert!(response.headers().get("connection").unwrap() == "upgrade");
        assert!(response.headers().get("upgrade").unwrap() == "websocket");
    }

    #[tokio::test]
    async fn test_accept_websocket_connection_additional_headers() {
        let handler = WebsocketHandler::new(Duration::from_secs(5), 1000, "client".into());
        let request = build_websocket_request();

        let send_to = SendToAppCollector::new();
        let receive_from =
            DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
                None,
                vec![(Bytes::from("X-Custom-Header"), Bytes::from("CustomValue"))],
            ))]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_ok());

        let response = response.unwrap();

        assert!(response.headers().get("X-Custom-Header").unwrap() == "CustomValue");
        assert!(response.headers().len() == 4);
    }

    #[tokio::test]
    async fn test_duplicate_headers() {
        let handler = WebsocketHandler::new(Duration::from_secs(5), 1000, "client".into());
        let request = build_websocket_request();

        let send_to = SendToAppCollector::new();
        let receive_from =
            DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
                None,
                vec![(
                    Bytes::from("sec-websocket-accept"),
                    Bytes::from("CustomValue"),
                )],
            ))]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_ok());
        assert!(response.unwrap().headers().len() == 4);
    }
}

#[cfg(test)]
mod test_frame_builder {
    use asgispec::events::WebsocketSendEvent;
    use bytes::Bytes;
    use fastwebsockets::OpCode;

    use super::FrameBuilder;

    fn text_event(s: &str) -> WebsocketSendEvent {
        WebsocketSendEvent::new(None, Some(s.to_string()))
    }

    fn binary_event(b: &[u8]) -> WebsocketSendEvent {
        WebsocketSendEvent::new(Some(Bytes::copy_from_slice(b)), None)
    }

    fn empty_event() -> WebsocketSendEvent {
        WebsocketSendEvent::new(None, None)
    }

    #[test]
    fn test_single_text_frame() {
        let builder = FrameBuilder::new(1024);
        let frames = builder.build(text_event("hello"));
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(&frames[0].payload[..], b"hello");
    }

    #[test]
    fn test_single_binary_frame() {
        let builder = FrameBuilder::new(1024);
        let frames = builder.build(binary_event(b"hello"));
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Binary);
        assert_eq!(&frames[0].payload[..], b"hello");
    }

    #[test]
    fn test_empty_event_returns_single_empty_binary_frame() {
        let builder = FrameBuilder::new(1024);
        let frames = builder.build(empty_event());
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Binary);
        assert_eq!(frames[0].payload.len(), 0);
    }

    #[test]
    fn test_data_exactly_at_max_frame_size_is_one_frame() {
        let builder = FrameBuilder::new(5);
        let frames = builder.build(text_event("hello"));
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Text);
    }

    #[test]
    fn test_data_one_byte_over_max_frame_size_splits_into_two_frames() {
        let builder = FrameBuilder::new(5);
        let frames = builder.build(text_event("hello!"));
        assert_eq!(frames.len(), 2);
        assert!(!frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(&frames[0].payload[..], b"hello");
        assert!(frames[1].fin);
        assert_eq!(frames[1].opcode, OpCode::Continuation);
        assert_eq!(&frames[1].payload[..], b"!");
    }

    #[test]
    fn test_text_splits_into_multiple_frames_with_correct_opcodes_and_fin() {
        let builder = FrameBuilder::new(4);
        // "abcdefghij" -> "abcd", "efgh", "ij" -> 3 frames
        let frames = builder.build(text_event("abcdefghij"));
        assert_eq!(frames.len(), 3);
        // First frame: original opcode, no FIN
        assert!(!frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(&frames[0].payload[..], b"abcd");
        // Middle frame: Continuation, no FIN
        assert!(!frames[1].fin);
        assert_eq!(frames[1].opcode, OpCode::Continuation);
        assert_eq!(&frames[1].payload[..], b"efgh");
        // Last frame: Continuation, FIN set
        assert!(frames[2].fin);
        assert_eq!(frames[2].opcode, OpCode::Continuation);
        assert_eq!(&frames[2].payload[..], b"ij");
    }

    #[test]
    fn test_binary_splits_into_multiple_frames() {
        let builder = FrameBuilder::new(3);
        let frames = builder.build(binary_event(b"abcdefgh"));
        // "abc", "def", "gh" -> 3 frames
        assert_eq!(frames.len(), 3);
        assert!(!frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Binary);
        assert_eq!(&frames[0].payload[..], b"abc");
        assert!(!frames[1].fin);
        assert_eq!(frames[1].opcode, OpCode::Continuation);
        assert_eq!(&frames[1].payload[..], b"def");
        assert!(frames[2].fin);
        assert_eq!(frames[2].opcode, OpCode::Continuation);
        assert_eq!(&frames[2].payload[..], b"gh");
    }

    #[test]
    fn test_text_takes_priority_over_bytes_when_both_present() {
        let builder = FrameBuilder::new(1024);
        let event = WebsocketSendEvent::new(Some(Bytes::from("binary")), Some("text".to_string()));
        let frames = builder.build(event);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(&frames[0].payload[..], b"text");
    }
}