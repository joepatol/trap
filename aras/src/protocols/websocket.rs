use std::collections::VecDeque;
use std::fmt::Display;
use std::result::Result as StdResult;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use asgispec::events::{WebsocketAcceptEvent, WebsocketCloseEvent, WebsocketSendEvent};
use asgispec::prelude::*;
use bytes::Bytes;
use bytes::BytesMut;
use derive_more::derive::Constructor;
use fastwebsockets::upgrade::UpgradeFut;
use fastwebsockets::{upgrade, FragmentCollector, Frame, OpCode, Payload, WebSocketError, CloseCode};
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

#[derive(Constructor)]
struct FrameBuilder {
    _max_message_size: u64,
    _max_frame_size: usize,
}

impl FrameBuilder {
    fn build_frames(&self, asgi_event: WebsocketSendEvent) -> VecDeque<Frame<'static>> {
        // TODO: implement fragmentation based on max_message_size and max_frame_size
        // TODO: check for max message size should be moved
        let mut frames = VecDeque::new();
        if let Some(data) = asgi_event.text {
            let payload = Payload::Owned(data.into_bytes());
            let frame = Frame::new(true, OpCode::Text, None, payload);
            frames.push_back(frame);
        }
        if let Some(data) = asgi_event.bytes {
            let payload = Payload::Bytes(BytesMut::from(&data[..]));
            let frame = Frame::new(true, OpCode::Binary, None, payload);
            frames.push_back(frame);
        }
        frames
    }
}

static FRAME_BUILDER: OnceLock<FrameBuilder> = OnceLock::new();

#[derive(Constructor)]
pub(crate) struct WebsocketHandler {
    timeout: Duration,
    max_message_size: u64,
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
        FRAME_BUILDER.get_or_init(|| FrameBuilder::new(self.max_message_size, self.max_frame_size));

        let app_response = self.accept(&mut send_to_app, &mut receive_from_app).await?;
        if !app_response.accepted() {
            return Ok(app_response.into());
        };

        let (upgrade_response, upgrade_future) = upgrade::upgrade(&mut request)?;
        self.run_protocol(upgrade_future, send_to_app, receive_from_app);

        merge_responses(app_response.into(), upgrade_response)
    }

    async fn accept(
        &self,
        send_to: &mut impl SendToASGIApp,
        receive_from: &mut impl ReceiveFromASGIApp,
    ) -> ArasResult<WsConnectResponse> {
        send_to.send(ASGIReceiveEvent::new_websocket_connect()).await?;
        receive_from.receive().await?.try_into()
    }

    fn run_protocol(
        self,
        upgrade_future: UpgradeFut,
        send_to_app: impl SendToASGIApp + 'static,
        receive_from_app: impl ReceiveFromASGIApp + 'static,
    ) {
        tokio::task::spawn(async move {
            let ws = match upgrade_future.await {
                // ASGI requires unfragmented messages to the application. Hence, fragment collector is the simplest.
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
        });
    }
}

#[derive(Constructor)]
struct WebsocketEventLoop {
    timeout: Duration,
}

impl WebsocketEventLoop {
    pub async fn run(
        &self,
        mut ws: FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>,
        mut send_to_app: impl SendToASGIApp,
        mut receive_from_app: impl ReceiveFromASGIApp,
    ) -> ArasResult<()> {
        let mut executed_event;
        let mut event = WebsocketEvent::Start;

        while !matches!(event, WebsocketEvent::Closed) {
            // TODO: when event execution fails we break and return an error.
            // Should we send close messages to both client and ASGI app as well?
            executed_event = event.execute(&mut ws, &mut send_to_app).await?;
            event = self.get_next_event(executed_event, &mut ws, &mut receive_from_app).await;
        }

        Ok(())
    }
    
    async fn get_next_event(
        &self,
        event: ExecutedEvent,
        ws: &mut FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>,
        receive_from_app: &mut impl ReceiveFromASGIApp,
    ) -> WebsocketEvent<'_> {
        if let ExecutedEvent::Closed = event {
            return WebsocketEvent::Closed;
        }

        if let ExecutedEvent::Closing = event {
            return WebsocketEvent::Closed;
        }

        tokio::select! {
            out = ws.read_frame() => WebsocketEvent::from(out),
            out = tokio::time::timeout(self.timeout, receive_from_app.receive()) => WebsocketEvent::from(out),
        }
    }
}   

/// Determines next step when receiving either a frame from the client or an ASGI event from the application
enum WebsocketEvent<'a> {
    // Nothing happened yet, need to start
    Start,
    // Send some frames to the client
    Frames(VecDeque<Frame<'a>>),
    // Send an event to the ASGI application
    ASGIEvent(ASGIReceiveEvent),
    // We're closing the connection, send close event and close frame
    Closing(ASGIReceiveEvent, Frame<'a>),
    // Closed
    Closed,
}

enum ExecutedEvent {
    Start,
    Frames,
    ASGIEvent,
    Closing,
    Closed,
}

impl From<&WebsocketEvent<'_>> for ExecutedEvent {
    fn from(value: &WebsocketEvent<'_>) -> Self {
        match value {
            WebsocketEvent::Start => ExecutedEvent::Start,
            WebsocketEvent::Frames(_) => ExecutedEvent::Frames,
            WebsocketEvent::ASGIEvent(_) => ExecutedEvent::ASGIEvent,
            WebsocketEvent::Closing(_, _) => ExecutedEvent::Closing,
            WebsocketEvent::Closed => ExecutedEvent::Closed,
        }
    }
}

impl<'a> WebsocketEvent<'a> {
    async fn execute(
        self,
        ws: &mut FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>,
        send_to_app: &mut impl SendToASGIApp,
    ) -> ArasResult<ExecutedEvent> {
        let executed_event = ExecutedEvent::from(&self);

        match self {
            WebsocketEvent::Frames(mut frames) => {
                while let Some(frame) = frames.pop_front() {
                    ws.write_frame(frame).await?;
                };
            }
            WebsocketEvent::ASGIEvent(asgi_event) => {
                send_to_app.send(asgi_event).await?;
            }
            WebsocketEvent::Closing(asgi_event, frame) => {
                ws.write_frame(frame).await?;
                send_to_app.send(asgi_event).await?;
            },
            _ => {}
        };

        Ok(executed_event)
    }
}

/// State transitions when receiving a frame from the client
impl<'a> From<StdResult<Frame<'a>, WebSocketError>> for WebsocketEvent<'a> {
    fn from(value: StdResult<Frame<'a>, WebSocketError>) -> Self {
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

impl From<Frame<'_>> for WebsocketEvent<'_> {
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
                Self::ASGIEvent(asgi_event)
            },
            OpCode::Binary => {
                let asgi_event = ASGIReceiveEvent::new_websocket_receive(bytes, None);
                Self::ASGIEvent(asgi_event)
            },
            OpCode::Close => {
                let data = bytes.unwrap_or(Bytes::new());
                let text = String::from_utf8_lossy(&data);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(CloseCode::Normal.into(), text.into());
                let frame = Frame::close(CloseCode::Normal.into(), "Client closed connection".as_bytes());
                Self::Closing(asgi_event, frame)
            },
            _ => {
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(CloseCode::Error.into(), String::from("unexpected opcode"));
                let frame = Frame::close(CloseCode::Error.into(), "Unexpected opcode".as_bytes());
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

/// State transitions when receiving an ASGI event from the application
impl From<StdResult<ArasResult<ASGISendEvent>, Elapsed>> for WebsocketEvent<'_> {
    fn from(value: StdResult<ArasResult<ASGISendEvent>, Elapsed>) -> Self {
        match value {
            Ok(msg) => Self::from(msg),
            Err(_) => {
                let payload = Payload::Owned(String::from("Internal server error").into_bytes());
                let frame = Frame::new(true, OpCode::Close, None, payload);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(), 
                    format!("Timeout receiving ASGI message in websocket loop")
                );
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

impl From<ArasResult<ASGISendEvent>> for WebsocketEvent<'_> {
    fn from(value: ArasResult<ASGISendEvent>) -> Self {
        match value {
            Ok(msg) => Self::from(msg),
            Err(e) => {
                let payload = Payload::Owned(String::from("Internal server error").into_bytes());
                let frame = Frame::new(true, OpCode::Close, None, payload);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(), 
                    format!("Error receiving ASGI message in websocket loop: {e}")
                );
                Self::Closing(asgi_event, frame)
            }
        }
    }
}

impl From<ASGISendEvent> for WebsocketEvent<'_> {
    fn from(value: ASGISendEvent) -> Self {
        match value {
            ASGISendEvent::WebsocketSend(msg) => {
                let builder = FRAME_BUILDER.get().expect("FrameBuilder not initialized");
                let frames = builder.build_frames(msg);
                Self::Frames(frames)
            }
            ASGISendEvent::WebsocketClose(msg) => {
                let payload = Payload::Owned(msg.reason.into_bytes());
                let frame = Frame::new(true, OpCode::Close, None, payload);
                let mut frames = VecDeque::new();
                frames.push_back(frame);
                Self::Frames(frames)
            }
            event => {
                let payload = Payload::Owned(String::from("Internal server error").into_bytes());
                let frame = Frame::new(true, OpCode::Close, None, payload);
                let asgi_event = ASGIReceiveEvent::new_websocket_disconnect(
                    CloseCode::Error.into(), 
                    format!("Received invalid ASGI message in websocket loop. Got: {event:?}")
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
//     use super::WebsocketHandler;

//     use std::sync::Arc;
//     use std::vec;
//     use tokio::sync::Mutex;

//     use asgispec::prelude::*;
//     use fastwebsockets::{FragmentCollector, Role, WebSocket};
//     use http::Request;

//     use crate::mocks::communication::{DeterministicReceiveFromApp, SendToAppCollector};
//     use crate::mocks::stream::MockWebsocketStream;

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
//     ) -> (MockWebsocketStream, Arc<Mutex<FragmentCollector<MockWebsocketStream>>>) {
//         let stream;
//         if messages.is_some() {
//             stream = MockWebsocketStream::new(messages.unwrap());
//         } else {
//             stream = MockWebsocketStream::empty();
//         }
//         let ws = WebSocket::after_handshake(stream.clone(), Role::Client);
//         let arc_ws = Arc::new(Mutex::new(FragmentCollector::new(ws)));
//         (stream, arc_ws)
//     }

//     #[tokio::test]
//     async fn test_asgi_messages_are_send_to_client() {
//         let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
//         let (stream, ws) = build_websocket_client(None);

//         let mut send_to = SendToAppCollector::new();
//         let mut receive_from = DeterministicReceiveFromApp::new(vec![
//             Ok(ASGISendEvent::new_websocket_send(None, Some("hello client".into()))),
//             Ok(ASGISendEvent::new_websocket_send(None, Some("im the server".into()))),
//             Ok(ASGISendEvent::new_websocket_close(None, "goodbye".into())),
//         ]);

//         let result = handler.run_protocol(&mut send_to, &mut receive_from, ws).await;

//         assert!(result.is_ok());

//         let written_to_stream = stream.written_unmasked().unwrap();
//         assert!(written_to_stream.len() == 3);
//         assert!(written_to_stream[0] == "hello client");
//         assert!(written_to_stream[1] == "im the server");
//         assert!(written_to_stream[2] == "goodbye");

//         let send_to_app = send_to.get_messages().await;
//         assert!(send_to_app.len() == 1);
//         assert!(send_to_app[0] == ASGIReceiveEvent::new_websocket_disconnect(1005));
//     }

//     #[tokio::test]
//     async fn test_client_messages_are_send_to_asgi_app() {
//         let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
//         let (_, ws) = build_websocket_client(Some(vec!["hello server", "im the client"]));

//         let mut send_to = SendToAppCollector::new();
//         let mut receive_from = DeterministicReceiveFromApp::new(vec![]);

//         let result = handler.run_protocol(&mut send_to, &mut receive_from, ws).await;
//         println!("{:?}", result);
//         assert!(result.is_ok());

//         let send_to_app = send_to.get_messages().await;
//         assert!(send_to_app.len() == 3);
//         assert!(send_to_app[0] == ASGIReceiveEvent::new_websocket_receive(None, Some("hello server".into())));
//         assert!(send_to_app[1] == ASGIReceiveEvent::new_websocket_receive(None, Some("im the client".into())));
//     }

//     #[tokio::test]
//     async fn test_accept_websocket_connection() {
//         let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
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
//         let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
//         let request = build_websocket_request();

//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
//             None,
//             vec![(b"X-Custom-Header".to_vec(), b"CustomValue".to_vec())],
//         ))]);

//         let response = handler.handle(send_to, receive_from, request).await;

//         assert!(response.is_ok());

//         let response = response.unwrap();

//         assert!(response.headers().get("X-Custom-Header").unwrap() == "CustomValue");
//         assert!(response.headers().len() == 4);
//     }

//     #[tokio::test]
//     async fn test_duplicate_headers() {
//         let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
//         let request = build_websocket_request();

//         let send_to = SendToAppCollector::new();
//         let receive_from = DeterministicReceiveFromApp::new(vec![Ok(ASGISendEvent::new_websocket_accept(
//             None,
//             vec![(b"sec-websocket-accept".to_vec(), b"CustomValue".to_vec())],
//         ))]);

//         let response = handler.handle(send_to, receive_from, request).await;

//         assert!(response.is_ok());
//         assert!(response.unwrap().headers().len() == 4);
//     }
// }
