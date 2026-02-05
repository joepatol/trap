use std::collections::VecDeque;
use std::fmt::Display;
use std::result::Result as StdResult;
use std::sync::Arc;
use std::time::Duration;

use asgispec::events::{WebsocketAcceptEvent, WebsocketCloseEvent, WebsocketSendEvent};
use asgispec::prelude::*;
use bytes::Bytes;
use derive_more::derive::Constructor;
use fastwebsockets::upgrade::UpgradeFut;
use fastwebsockets::{upgrade, CloseCode, FragmentCollector, Frame, OpCode, Payload, WebSocketError};
use http::StatusCode;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Body;
use hyper::Request;
use log::error;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::types::Response;
use crate::{ArasError, ArasResult};

#[derive(Constructor)]
pub(crate) struct WebsocketHandler {
    timeout: Duration,
    max_frame_size: usize,
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
        send_to.send(ASGIReceiveEvent::new_websocket_connect()).await?;
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
        let mut ws = match upgrade_future.await {
            // ASGI requires unfragmented messages to the application. So just use FragmentCollector.
            Ok(ws) => FragmentCollector::new(ws),
            Err(e) => {
                error!("Websocket upgrade failed: {}", e);
                return;
            }
        };

        let mut state = State::Starting;
        let mut context = Context::new(self.max_frame_size, send_to_app, receive_from_app);

        while !matches!(state, State::Closed) {
            let event = Event::next(&mut ws, &mut context).await;
            state = state.on_event(event, &mut ws, &mut context).await;
        }
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
            msg => Err(ArasError::unexpected_asgi_message(Arc::new(msg))),
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
        if value.subprotocol.is_some() {
            builder = builder.header(hyper::header::SEC_WEBSOCKET_PROTOCOL, value.subprotocol.unwrap())
        };
        for (bytes_key, bytes_value) in value.headers.into_iter() {
            builder = builder.header(bytes_key.to_vec(), bytes_value.to_vec());
        }
        Ok(Self::Accept(builder.body(body)?))
    }
}

impl TryFrom<WebsocketCloseEvent> for ConnectResponse {
    type Error = ArasError;

    fn try_from(value: WebsocketCloseEvent) -> Result<Self, Self::Error> {
        let body = Full::new(value.reason.into()).map_err(|never| match never {}).boxed();
        let mut builder = http::Response::builder();
        builder = builder.status(StatusCode::FORBIDDEN);
        Ok(Self::Close(builder.body(body)?))
    }
}

enum State {
    Starting,
    Running,
    Error(ArasError),
    Closing(u16, String),
    Closed,
}

impl State {
    pub async fn on_event<W, S, R>(&self, event: Event, ws: &mut FragmentCollector<W>, ctx: &mut Context<S, R>) -> Self 
    where
        W: AsyncRead + AsyncWrite + Unpin,
        S: SendToASGIApp,
        R: ReceiveFromASGIApp,
    {   
        let next_state = match event {
            Event::FrameReceived(frame) => self.on_frame_received(frame, ctx).await,
            Event::ASGIEventReceived(asgi) => self.on_asgi_received(asgi, ws, ctx).await,
            Event::ErrorOccurred(error) => self.on_error(error).await,
        };

        match next_state {
            State::Error(e) => self.on_error(e.clone()).await,
            State::Closing(code, msg) => self.close(code.clone(), msg.clone(), ws, ctx).await,
            s => s,
        }
    }

    async fn close<W, S, R>(&self, code: u16, msg: String, ws: &mut FragmentCollector<W>, ctx: &mut Context<S, R>) -> Self 
    where 
        W: AsyncRead + AsyncWrite + Unpin,
        S: SendToASGIApp,
        R: ReceiveFromASGIApp,
    {
        let frame = Frame::close(code.into(), msg.as_bytes());
        let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(code.into(), msg.into());
        let _ = ws.write_frame(frame).await;
        let _ = ctx.send_to_app(asgi_event).await;
        Self::Closed
    }

    async fn on_frame_received<S, R>(&self, frame: ASGIReceiveEvent, ctx: &mut Context<S, R>) -> Self 
    where
        S: SendToASGIApp,
        R: ReceiveFromASGIApp,
    {
        if let ASGIReceiveEvent::WebsocketDisconnect(msg) = frame {
            return Self::Closing(msg.code, msg.reason)
        }

        match ctx.send_to_app(frame).await {
            Ok(_) => Self::Running,
            Err(e) => Self::Error(e),
        }
    }

    async fn on_asgi_received<W, S, R>(&self, asgi: ASGISendEvent, ws: &mut FragmentCollector<W>, ctx: &mut Context<S, R>) -> Self 
    where
        W: AsyncRead + AsyncWrite + Unpin,
        S: SendToASGIApp,
        R: ReceiveFromASGIApp,
    {
        let ws_send = match asgi {
            ASGISendEvent::WebsocketSend(msg) => msg,
            ASGISendEvent::WebsocketClose(msg) => return Self::Closing(msg.code, msg.reason),
            _ => return Self::Error(ArasError::unexpected_asgi_message(Arc::new(asgi))),
        };
        let frames = ctx.frame_builder.build(ws_send);
        for frame in frames {
            if let Err(e) = ws.write_frame(frame).await {
                return Self::Error(e.into());
            }
        };
        Self::Running
    }

    async fn on_error(&self, error: ArasError) -> Self {
        error!("Error in websocket loop: {}", error);
        Self::Closing(CloseCode::Error.into(), "Internal server error".into())
    }
}

enum Event {
    FrameReceived(ASGIReceiveEvent),
    ASGIEventReceived(ASGISendEvent),
    ErrorOccurred(ArasError),
}

impl Event {
    pub async fn next<W, S, R>(ws: &mut FragmentCollector<W>, ctx: &mut Context<S, R>) -> Self
    where
        W: AsyncRead + AsyncWrite + Unpin,
        S: SendToASGIApp,
        R: ReceiveFromASGIApp,
    {
        tokio::select! {
            out = ws.read_frame() => Event::from(out),
            out = ctx.receive_from_app() => Event::from(out),
        }
    }
}

struct Context<S, R> 
where
    S: SendToASGIApp,
    R: ReceiveFromASGIApp,
{
    frame_builder: FrameBuilder,
    send_to_app: S,
    receive_from_app: R,
}

impl<S, R> Context<S, R> 
where
    S: SendToASGIApp,
    R: ReceiveFromASGIApp,
{
    pub fn new(
        max_frame_size: usize,
        send_to_app: S,
        receive_from_app: R,
    ) -> Self {
        let frame_builder = FrameBuilder::new(max_frame_size);
        Self {
            frame_builder,
            send_to_app,
            receive_from_app,
        }
    }

    pub async fn send_to_app(&mut self, event: ASGIReceiveEvent) -> ArasResult<()> {
        self.send_to_app.send(event).await
    }

    pub async fn receive_from_app(&mut self) -> ArasResult<ASGISendEvent> {
        self.receive_from_app.receive().await
    }
}

impl From<StdResult<Frame<'_>, WebSocketError>> for Event {
    fn from(value: StdResult<Frame<'_>, WebSocketError>) -> Self {
        match value {
            Ok(frame) => Self::from(frame),
            Err(e) => Self::ErrorOccurred(e.into())
        }
    }
}

impl From<Frame<'_>> for Event {
    fn from(value: Frame<'_>) -> Self {
        let bytes = match value.payload {
            Payload::Bytes(b) => Some(Bytes::from(b)),
            Payload::Owned(b) => Some(Bytes::from(b)),
            Payload::Borrowed(b) => Some(Bytes::copy_from_slice(b).into()),
            Payload::BorrowedMut(b) => Some(Bytes::copy_from_slice(b).into()),
        };

        match value.opcode {
            OpCode::Text => {
                let data = bytes.unwrap_or(Bytes::new());
                let text = String::from_utf8_lossy(&data);
                let asgi_event = ASGIReceiveEvent::new_websocket_receive(None, Some(text.into()));
                Self::FrameReceived(asgi_event)
            }
            OpCode::Binary => {
                let asgi_event = ASGIReceiveEvent::new_websocket_receive(bytes, None);
                Self::FrameReceived(asgi_event)
            }
            OpCode::Close => {
                let data = bytes.unwrap_or(Bytes::new());
                let text = String::from_utf8_lossy(&data);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(CloseCode::Normal.into(), text.into());
                Self::FrameReceived(asgi_event)
            }
            op_code => {
                Self::ErrorOccurred(ArasError::custom(format!("unexpected opcode: {op_code:?}")))
            }
        }
    }
}

impl From<ArasResult<ASGISendEvent>> for Event {
    fn from(value: ArasResult<ASGISendEvent>) -> Self {
        match value {
            Ok(msg) => Self::from(msg),
            Err(e) => Self::ErrorOccurred(e),
        }
    }
}

impl From<ASGISendEvent> for Event {
    fn from(value: ASGISendEvent) -> Self {
        match value {
            ASGISendEvent::WebsocketSend(msg) => Self::ASGIEventReceived(ASGISendEvent::WebsocketSend(msg)),
            ASGISendEvent::WebsocketClose(msg) => Self::ASGIEventReceived(ASGISendEvent::WebsocketClose(msg)),
            event => Self::ErrorOccurred(ArasError::unexpected_asgi_message(Arc::new(event))),
        }
    }
}  

/// Build websocket frames from ASGI websocket send events.
/// Frames are fragmented according to the max_frame_size.
#[derive(Constructor)]
struct FrameBuilder {
    max_frame_size: usize,
}

impl FrameBuilder {
    fn build(&self, asgi_event: WebsocketSendEvent) -> VecDeque<Frame<'static>> {
        let chunks;
        let op_code;
        let mut frames = VecDeque::new();

        if let Some(data) = asgi_event.text {
            chunks = self.split_string(data);
            op_code = OpCode::Text;
        } else if let Some(data) = asgi_event.bytes {
            chunks = self.split_bytes(data);
            op_code = OpCode::Binary;
        } else {
            chunks = Vec::new();
            op_code = OpCode::Binary;
        }

        let frame_count = chunks.len().max(1);

        let first_frame = Frame::new(
            frame_count == 1,
            op_code,
            None,
            Payload::Owned(chunks.first().unwrap_or(&Bytes::new()).to_vec()),
        );
        frames.push_back(first_frame);

        if frame_count == 1 {
            return frames;
        }

        if frame_count > 2 {
            for chunk in chunks.iter().skip(1).take(frame_count - 2) {
                let frame = Frame::new(false, OpCode::Continuation, None, Payload::Owned(chunk.to_vec()));
                frames.push_back(frame);
            }
        }

        let last_frame = Frame::new(
            true,
            OpCode::Continuation,
            None,
            Payload::Owned(chunks.last().unwrap_or(&Bytes::new()).to_vec()),
        );
        frames.push_back(last_frame);

        frames
    }

    fn split_bytes(&self, data: Bytes) -> Vec<Bytes> {
        if data.len() <= self.max_frame_size {
            return vec![data];
        };

        let mut chunks = Vec::new();
        let mut start = 0;
        while start < data.len() {
            let end = std::cmp::min(start + self.max_frame_size, data.len());
            chunks.push(data.slice(start..end));
            start = end;
        }
        chunks
    }

    fn split_string(&self, data: String) -> Vec<Bytes> {
        let bytes = data.into_bytes();
        self.split_bytes(Bytes::from(bytes))
    }
}

fn merge_responses(app_response: Response, upgrade_response: http::Response<Empty<Bytes>>) -> ArasResult<Response> {
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

// #[cfg(test)]
// mod tests {
//     use super::{FrameBuilder, WebsocketEventLoop, WebsocketHandler};

//     use std::time::Duration;
//     use std::vec;

//     use asgispec::prelude::*;
//     use bytes::Bytes;
//     use fastwebsockets::{FragmentCollector, Role, WebSocket};
//     use http::Request;

//     use crate::mocks::communication::{DeterministicReceiveFromApp, SendToAppCollector};
//     use crate::mocks::stream::MockWebsocketStream;
//     use crate::protocols::websocket::FRAME_BUILDER;

//     fn build_websocket_request() -> Request<String> {
//         Request::builder()
//             .header("Connection", "Upgrade")
//             .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
//             .header("sec-websocket-version", "13")
//             .body("".to_string())
//             .expect("Failed to build request")
//     }

//     fn build_websocket_client(
//         messages: Option<Vec<&str>>,
//     ) -> (MockWebsocketStream, FragmentCollector<MockWebsocketStream>) {
//         let stream;
//         if messages.is_some() {
//             stream = MockWebsocketStream::new(messages.unwrap());
//         } else {
//             stream = MockWebsocketStream::empty();
//         }
//         let ws = WebSocket::after_handshake(stream.clone(), Role::Client);
//         let arc_ws = FragmentCollector::new(ws);
//         (stream, arc_ws)
//     }

//     #[tokio::test]
//     async fn test_asgi_messages_are_send_to_client() {
//         FRAME_BUILDER.get_or_init(|| FrameBuilder::new(1024));
//         let (stream, ws) = build_websocket_client(None);
//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![
//             Ok(ASGISendEvent::new_websocket_send(None, Some("hello client".into()))),
//             Ok(ASGISendEvent::new_websocket_send(None, Some("im the server".into()))),
//             Ok(ASGISendEvent::new_websocket_close(1000, "goodbye".into())),
//         ]);
//         let event_loop = WebsocketEventLoop::new();

//         let result = event_loop.run(ws, send_to.clone(), receive_from).await;

//         assert!(result.is_ok());

//         let written_to_stream = stream.written_unmasked().unwrap();
//         assert!(written_to_stream.len() == 3);
//         assert!(written_to_stream[0] == "hello client");
//         assert!(written_to_stream[1] == "im the server");
//         assert!(written_to_stream[2] == "goodbye");

//         let send_to_app = send_to.get_messages().await;
//         assert!(send_to_app.len() == 1);
//         assert!(matches!(send_to_app[0], ASGIReceiveEvent::WebsocketDisconnect(_)));
//     }

//     #[tokio::test]
//     async fn test_frames_split_when_exceeding_max_message_size() {
//         FRAME_BUILDER.get_or_init(|| FrameBuilder::new(10));
//         let (stream, ws) = build_websocket_client(None);
//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![
//             Ok(ASGISendEvent::new_websocket_send(
//                 None,
//                 Some("I would very much like to be split up".into()),
//             )),
//             Ok(ASGISendEvent::new_websocket_close(1000, "goodbye".into())),
//         ]);
//         let event_loop = WebsocketEventLoop::new();

//         let result = event_loop.run(ws, send_to.clone(), receive_from).await;

//         assert!(result.is_ok());

//         let written_to_stream = stream.written_unmasked().unwrap();
//         assert!(written_to_stream.len() == 5);
//         assert!(written_to_stream[0] == "I would ve");
//         assert!(written_to_stream[3] == "plit up");
//     }

//     #[tokio::test]
//     async fn test_client_messages_are_send_to_asgi_app() {
//         let (_, ws) = build_websocket_client(Some(vec!["hello server", "im the client"]));
//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![]);

//         let event_loop = WebsocketEventLoop::new();

//         let result = event_loop.run(ws, send_to.clone(), receive_from).await;
//         assert!(result.is_ok());

//         let send_to_app = send_to.get_messages().await;
//         assert!(send_to_app.len() == 3);
//         assert!(send_to_app[0] == ASGIReceiveEvent::new_websocket_receive(None, Some("hello server".into())));
//         assert!(send_to_app[1] == ASGIReceiveEvent::new_websocket_receive(None, Some("im the client".into())));
//     }

//     #[tokio::test]
//     async fn test_accept_websocket_connection() {
//         let handler = WebsocketHandler::new(Duration::from_secs(5), 1000);
//         let request = build_websocket_request();

//         let send_to = SendToAppCollector::new();
//         let receive_from =
//             DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(None, vec![]))]);

//         let response = handler.handle(send_to, receive_from, request).await;

//         assert!(response.is_ok());

//         let response = response.unwrap();

//         assert!(response.status() == http::StatusCode::SWITCHING_PROTOCOLS);
//         assert!(response.headers().get("sec-websocket-accept").is_some());
//         assert!(response.headers().get("connection").unwrap() == "upgrade");
//         assert!(response.headers().get("upgrade").unwrap() == "websocket");
//     }

//     #[tokio::test]
//     async fn test_accept_websocket_connection_additional_headers() {
//         let handler = WebsocketHandler::new(Duration::from_secs(5), 1000);
//         let request = build_websocket_request();

//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
//             None,
//             vec![(Bytes::from("X-Custom-Header"), Bytes::from("CustomValue"))],
//         ))]);

//         let response = handler.handle(send_to, receive_from, request).await;

//         assert!(response.is_ok());

//         let response = response.unwrap();

//         assert!(response.headers().get("X-Custom-Header").unwrap() == "CustomValue");
//         assert!(response.headers().len() == 4);
//     }

//     #[tokio::test]
//     async fn test_duplicate_headers() {
//         let handler = WebsocketHandler::new(Duration::from_secs(5), 1000);
//         let request = build_websocket_request();

//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
//             None,
//             vec![(Bytes::from("sec-websocket-accept"), Bytes::from("CustomValue"))],
//         ))]);

//         let response = handler.handle(send_to, receive_from, request).await;

//         assert!(response.is_ok());
//         assert!(response.unwrap().headers().len() == 4);
//     }
// }