use std::collections::VecDeque;
use std::fmt::Display;
use std::result::Result as StdResult;
use std::sync::{Arc, OnceLock};
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
use tokio::time::error::Elapsed;

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::types::Response;
use crate::{ArasError, ArasResult};

static FRAME_BUILDER: OnceLock<FrameBuilder> = OnceLock::new();

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
        FRAME_BUILDER.get_or_init(|| FrameBuilder::new(self.max_frame_size));

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
    ) -> ArasResult<WsConnectResponse> {
        send_to.send(ASGIReceiveEvent::new_websocket_connect()).await?;
        tokio::time::timeout(self.timeout, receive_from.receive()).await??.try_into()
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

        let event_loop = WebsocketEventLoop::new(self.timeout);
        if let Err(e) = event_loop.run(ws, send_to_app, receive_from_app).await {
            error!("Websocket event loop terminated with error: {}", e);
        }
    }
}

#[derive(Constructor)]
struct WebsocketEventLoop {
    timeout: Duration,
}

impl WebsocketEventLoop {
    /// Run the websocket event loop until the connection is closed or state execution fails.
    /// When state execution fails an error is retuned, no close messages are sent to the client or ASGI app.
    pub async fn run(
        &self,
        mut ws: FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>,
        mut send_to_app: impl SendToASGIApp,
        mut receive_from_app: impl ReceiveFromASGIApp,
    ) -> ArasResult<()> {
        let mut executed_event;
        let mut state = WebsocketState::Starting;

        while !matches!(state, WebsocketState::Closed) {
            executed_event = state.execute(&mut ws, &mut send_to_app).await?;
            state = self.next(executed_event, &mut ws, &mut receive_from_app).await;
        }

        Ok(())
    }

    /// Determine the next state based on the executed event and incoming events
    /// from the client (websocket frames) and the ASGI application.
    async fn next(
        &self,
        state: ExecutedState,
        ws: &mut FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>,
        receive_from_app: &mut impl ReceiveFromASGIApp,
    ) -> WebsocketState<'_> {
        if let ExecutedState::Closed = state {
            return WebsocketState::Closed;
        }

        tokio::select! {
            out = ws.read_frame() => WebsocketState::from(out),
            out = tokio::time::timeout(self.timeout, receive_from_app.receive()) => WebsocketState::from(out),
        }
    }
}

/// States the websocket event loop can be in
enum WebsocketState<'a> {
    // Starting state, wait for first event
    Starting,
    // Got some frames from the app to send to the client
    SendFrames(VecDeque<Frame<'a>>),
    // Got frames from the client to send to the app
    SendASGIEvent(ASGIReceiveEvent),
    // Connection about to close, send close event and close frame
    Closing(ASGIReceiveEvent, Frame<'a>),
    // Connection is closed
    Closed,
}

/// Each state corresponds to some work to be done.
/// This enum represents the states after the work was done
enum ExecutedState {
    Started,
    FramesSend,
    ASGIEventSend,
    Closed,
}

impl<'a> WebsocketState<'a> {
    /// Execute the work associated with this state and return the
    /// resulting state. If executing the work fails an error is returned.
    async fn execute(
        self,
        ws: &mut FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>,
        send_to_app: &mut impl SendToASGIApp,
    ) -> ArasResult<ExecutedState> {
        let executed_event = ExecutedState::from(&self);

        match self {
            WebsocketState::SendFrames(mut frames) => {
                while let Some(frame) = frames.pop_front() {
                    ws.write_frame(frame).await?;
                }
            }
            WebsocketState::SendASGIEvent(asgi_event) => {
                send_to_app.send(asgi_event).await?;
            }
            WebsocketState::Closing(asgi_event, frame) => {
                ws.write_frame(frame).await?;
                send_to_app.send(asgi_event).await?;
            }
            _ => {}
        };

        Ok(executed_event)
    }
}

/// State transitions when executing a state
impl From<&WebsocketState<'_>> for ExecutedState {
    fn from(value: &WebsocketState<'_>) -> Self {
        match value {
            WebsocketState::Starting => ExecutedState::Started,
            WebsocketState::SendFrames(_) => ExecutedState::FramesSend,
            WebsocketState::SendASGIEvent(_) => ExecutedState::ASGIEventSend,
            WebsocketState::Closing(_, _) => ExecutedState::Closed,
            WebsocketState::Closed => ExecutedState::Closed,
        }
    }
}

/// State transitions when receiving a frame from the client
impl From<StdResult<Frame<'_>, WebSocketError>> for WebsocketState<'_> {
    fn from(value: StdResult<Frame<'_>, WebSocketError>) -> Self {
        match value {
            Ok(frame) => Self::from(frame),
            Err(e) => {
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(CloseCode::Error.into(), e.to_string());
                let frame = Frame::close(CloseCode::Error.into(), "internal server error".as_bytes());
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

impl From<Frame<'_>> for WebsocketState<'_> {
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
                Self::SendASGIEvent(asgi_event)
            }
            OpCode::Binary => {
                let asgi_event = ASGIReceiveEvent::new_websocket_receive(bytes, None);
                Self::SendASGIEvent(asgi_event)
            }
            OpCode::Close => {
                let data = bytes.unwrap_or(Bytes::new());
                let text = String::from_utf8_lossy(&data);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(CloseCode::Normal.into(), text.into());
                let frame = Frame::close(CloseCode::Normal.into(), "Client closed connection".as_bytes());
                Self::Closing(asgi_event, frame)
            }
            _ => {
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(),
                    String::from("unexpected opcode"),
                );
                let frame = Frame::close(CloseCode::Error.into(), "Unexpected opcode".as_bytes());
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

/// State transitions when receiving an ASGI event from the application
impl From<StdResult<ArasResult<ASGISendEvent>, Elapsed>> for WebsocketState<'_> {
    fn from(value: StdResult<ArasResult<ASGISendEvent>, Elapsed>) -> Self {
        match value {
            Ok(msg) => Self::from(msg),
            Err(_) => {
                let frame = Frame::close(CloseCode::Error.into(), "Internal server error".as_bytes());
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(),
                    format!("Timeout receiving ASGI message in websocket loop"),
                );
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

impl From<ArasResult<ASGISendEvent>> for WebsocketState<'_> {
    fn from(value: ArasResult<ASGISendEvent>) -> Self {
        match value {
            Ok(msg) => Self::from(msg),
            Err(e) => {
                let frame = Frame::close(CloseCode::Error.into(), "Internal server error".as_bytes());
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(),
                    format!("Error receiving ASGI message in websocket loop: {e}"),
                );
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

impl From<ASGISendEvent> for WebsocketState<'_> {
    fn from(value: ASGISendEvent) -> Self {
        match value {
            ASGISendEvent::WebsocketSend(msg) => {
                let builder = FRAME_BUILDER.get().expect("FrameBuilder not initialized");
                let frames = builder.build(msg);
                Self::SendFrames(frames)
            }
            // TODO: Can reason be large than max frame size?
            ASGISendEvent::WebsocketClose(msg) => {
                let payload = Payload::Owned(msg.reason.into_bytes());
                let frame = Frame::new(true, OpCode::Close, None, payload);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    msg.code.into(),
                    "Application send websocket.close".into(),
                );
                Self::Closing(asgi_event, frame)
            }
            event => {
                let payload = Payload::Owned(String::from("Internal server error").into_bytes());
                let frame = Frame::new(true, OpCode::Close, None, payload);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(),
                    format!("Received invalid ASGI message in websocket loop. \n {event}"),
                );
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

/// Result of attempting to accept a websocket connection
enum WsConnectResponse {
    Accept(Response),
    Close(Response),
}

impl WsConnectResponse {
    pub fn accepted(&self) -> bool {
        matches!(self, WsConnectResponse::Accept(_))
    }
}

impl From<WsConnectResponse> for Response {
    fn from(value: WsConnectResponse) -> Self {
        match value {
            WsConnectResponse::Accept(resp) => resp,
            WsConnectResponse::Close(resp) => resp,
        }
    }
}

impl TryFrom<ASGISendEvent> for WsConnectResponse {
    type Error = ArasError;

    fn try_from(value: ASGISendEvent) -> Result<Self, Self::Error> {
        match value {
            ASGISendEvent::WebsocketAccept(msg) => WsConnectResponse::try_from(msg),
            ASGISendEvent::WebsocketClose(msg) => WsConnectResponse::try_from(msg),
            msg => Err(ArasError::unexpected_asgi_message(Arc::new(msg))),
        }
    }
}

impl TryFrom<WebsocketAcceptEvent> for WsConnectResponse {
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
            builder = builder.header(bytes_key, bytes_value);
        }
        Ok(Self::Accept(builder.body(body)?))
    }
}

impl TryFrom<WebsocketCloseEvent> for WsConnectResponse {
    type Error = ArasError;

    fn try_from(value: WebsocketCloseEvent) -> Result<Self, Self::Error> {
        let body = Full::new(value.reason.into()).map_err(|never| match never {}).boxed();
        let mut builder = http::Response::builder();
        builder = builder.status(StatusCode::FORBIDDEN);
        Ok(Self::Close(builder.body(body)?))
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

/// Merge the ASGI application response and the websocket upgrade response.
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

#[cfg(test)]
mod tests {
    use super::{FrameBuilder, WebsocketEventLoop, WebsocketHandler};

    use std::time::Duration;
    use std::vec;

    use asgispec::prelude::*;
    use fastwebsockets::{FragmentCollector, Role, WebSocket};
    use http::Request;

    use crate::mocks::communication::{DeterministicReceiveFromApp, SendToAppCollector};
    use crate::mocks::stream::MockWebsocketStream;
    use crate::protocols::websocket::FRAME_BUILDER;

    fn build_websocket_request() -> Request<String> {
        Request::builder()
            .header("Connection", "Upgrade")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("sec-websocket-version", "13")
            .body("".to_string())
            .expect("Failed to build request")
    }

    fn build_websocket_client(
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

    #[tokio::test]
    async fn test_asgi_messages_are_send_to_client() {
        FRAME_BUILDER.get_or_init(|| FrameBuilder::new(1024));
        let (stream, ws) = build_websocket_client(None);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_websocket_send(None, Some("hello client".into()))),
            Ok(ASGISendEvent::new_websocket_send(None, Some("im the server".into()))),
            Ok(ASGISendEvent::new_websocket_close(1000, "goodbye".into())),
        ]);
        let event_loop = WebsocketEventLoop::new(Duration::from_secs(5));

        let result = event_loop.run(ws, send_to.clone(), receive_from).await;

        assert!(result.is_ok());

        let written_to_stream = stream.written_unmasked().unwrap();
        assert!(written_to_stream.len() == 3);
        assert!(written_to_stream[0] == "hello client");
        assert!(written_to_stream[1] == "im the server");
        assert!(written_to_stream[2] == "goodbye");

        let send_to_app = send_to.get_messages().await;
        assert!(send_to_app.len() == 1);
        assert!(matches!(send_to_app[0], ASGIReceiveEvent::WebsocketDisconnect(_)));
    }

    #[tokio::test]
    async fn test_client_messages_are_send_to_asgi_app() {
        let (_, ws) = build_websocket_client(Some(vec!["hello server", "im the client"]));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let event_loop = WebsocketEventLoop::new(Duration::from_secs(5));

        let result = event_loop.run(ws, send_to.clone(), receive_from).await;
        println!("{:?}", result);
        assert!(result.is_ok());

        let send_to_app = send_to.get_messages().await;
        assert!(send_to_app.len() == 3);
        assert!(send_to_app[0] == ASGIReceiveEvent::new_websocket_receive(None, Some("hello server".into())));
        assert!(send_to_app[1] == ASGIReceiveEvent::new_websocket_receive(None, Some("im the client".into())));
    }

    #[tokio::test]
    async fn test_accept_websocket_connection() {
        let handler = WebsocketHandler::new(std::time::Duration::from_secs(5), 1000);
        let request = build_websocket_request();

        let send_to = SendToAppCollector::new();
        let receive_from =
            DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(None, vec![]))]);

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
        let handler = WebsocketHandler::new(std::time::Duration::from_secs(5), 1000);
        let request = build_websocket_request();

        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
            None,
            vec![(b"X-Custom-Header".to_vec(), b"CustomValue".to_vec())],
        ))]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_ok());

        let response = response.unwrap();

        assert!(response.headers().get("X-Custom-Header").unwrap() == "CustomValue");
        assert!(response.headers().len() == 4);
    }

    #[tokio::test]
    async fn test_duplicate_headers() {
        let handler = WebsocketHandler::new(std::time::Duration::from_secs(5), 1000);
        let request = build_websocket_request();

        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
            None,
            vec![(b"sec-websocket-accept".to_vec(), b"CustomValue".to_vec())],
        ))]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_ok());
        assert!(response.unwrap().headers().len() == 4);
    }
}
