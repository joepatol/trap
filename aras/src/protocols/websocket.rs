use std::fmt::Display;
use std::result::Result as StdResult;
use std::sync::Arc;
use std::time::Duration;

use asgispec::prelude::*;
use bytes::Bytes;
use bytes::BytesMut;
use derive_more::derive::Constructor;
use fastwebsockets::{upgrade, FragmentCollector, Frame, OpCode, Payload, WebSocketError};
use http::StatusCode;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Body;
use hyper::Request;
use log::error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use tokio::time::error::Elapsed;

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::types::Response;
use crate::{ArasError, ArasResult};

#[derive(Constructor)]
pub(crate) struct WebsocketHandler {
    timeout: Duration,
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
        let (accepted, app_response) = self
            .accept_websocket_connection(&mut send_to_app, &mut receive_from_app)
            .await?;

        if !accepted {
            return Ok(app_response);
        };

        let (upgrade_response, fut) = upgrade::upgrade(&mut request)?;

        tokio::task::spawn(async move {
            let ws = match fut.await {
                Ok(ws) => Arc::new(Mutex::new(FragmentCollector::new(ws))),
                Err(e) => {
                    error!("Websocket upgrade failed: {}", e);
                    return;
                }
            };

            if let Err(e) = self
                .run_accepted_websocket(&mut send_to_app, &mut receive_from_app, ws)
                .await
            {
                error!("Error while serving websocket; {e}");
            }
        });

        merge_responses(app_response, upgrade_response)
    }

    async fn accept_websocket_connection(
        &self,
        send_to: &mut impl SendToASGIApp,
        receive_from: &mut impl ReceiveFromASGIApp,
    ) -> ArasResult<(bool, Response)> {
        let mut builder = hyper::Response::builder();
        send_to.send(ASGIReceiveEvent::new_websocket_connect()).await?;

        match tokio::time::timeout(self.timeout, receive_from.receive()).await {
            Ok(Ok(ASGISendEvent::WebsocketAccept(msg))) => {
                let body = Full::new(Vec::<u8>::new().into())
                    .map_err(|never| match never {})
                    .boxed();
                builder = builder.status(StatusCode::SWITCHING_PROTOCOLS);
                if msg.subprotocol.is_some() {
                    builder = builder.header(hyper::header::SEC_WEBSOCKET_PROTOCOL, msg.subprotocol.unwrap())
                };
                for (bytes_key, bytes_value) in msg.headers.into_iter() {
                    builder = builder.header(bytes_key, bytes_value);
                }
                Ok((true, builder.body(body)?))
            }
            Ok(Ok(ASGISendEvent::WebsocketClose(msg))) => {
                let body = Full::new(msg.reason.into()).map_err(|never| match never {}).boxed();
                builder = builder.status(StatusCode::FORBIDDEN);
                Ok((false, builder.body(body)?))
            }
            Ok(Err(e)) => Err(e),
            Ok(Ok(msg)) => Err(ArasError::unexpected_asgi_message(Arc::new(msg))),
            Err(e) => Err(e.into()),
        }
    }

    async fn run_accepted_websocket(
        &self,
        send_to: &mut impl SendToASGIApp,
        receive_from: &mut impl ReceiveFromASGIApp,
        ws: Arc<Mutex<FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>>>,
    ) -> ArasResult<()> {
        loop {
            let ws_iter = ws.clone();
            let mut ws_locked = ws_iter.lock().await;

            let iteration: WsIteration<'_> = tokio::select! {
                out = ws_locked.read_frame() => WsIteration::ReceiveClient(out),
                out = tokio::time::timeout(self.timeout, receive_from.receive()) => WsIteration::ReceiveApplication(out),
            };

            drop(ws_locked); // Drop the lock so it can be acquired for writing

            match iteration {
                WsIteration::ReceiveClient(frame) => {
                    if !do_server_iteration(frame?, send_to).await? {
                        break;
                    };
                }
                WsIteration::ReceiveApplication(msg) => {
                    let ws_clone = ws.clone();
                    if !do_app_iteration(msg, ws_clone).await? {
                        break;
                    };
                }
            };
        }

        send_to.send(ASGIReceiveEvent::new_websocket_disconnect(1005)).await?;

        Ok(())
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

enum WsIteration<'a> {
    ReceiveClient(StdResult<fastwebsockets::Frame<'a>, WebSocketError>),
    ReceiveApplication(StdResult<ArasResult<ASGISendEvent>, Elapsed>),
}

async fn do_app_iteration(
    msg: StdResult<ArasResult<ASGISendEvent>, Elapsed>,
    ws: Arc<Mutex<FragmentCollector<impl AsyncRead + AsyncWrite + Unpin>>>,
) -> ArasResult<bool> {
    match msg {
        Ok(Ok(ASGISendEvent::WebsocketSend(msg))) => {
            if let Some(data) = msg.text {
                let payload = Payload::Owned(data.into_bytes());
                let frame = Frame::new(true, OpCode::Text, None, payload);
                ws.lock().await.write_frame(frame).await?;
            }
            if let Some(data) = msg.bytes {
                let payload = Payload::Bytes(BytesMut::from(&data[..]));
                let frame = Frame::new(true, OpCode::Binary, None, payload);
                ws.lock().await.write_frame(frame).await?;
            }
            Ok(true)
        }
        Ok(Ok(ASGISendEvent::WebsocketClose(msg))) => {
            let payload = Payload::Owned(msg.reason.into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Ok(false)
        }
        Ok(Err(e)) => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(e)
        }
        Ok(Ok(msg)) => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(ArasError::unexpected_asgi_message(Arc::new(format!(
                "Received invalid ASGI message in websocket loop. Got: {msg:?}"
            ))))
        }
        Err(e) => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(e.into())
        }
    }
}

async fn do_server_iteration(frame: Frame<'_>, send_to: &mut impl SendToASGIApp) -> ArasResult<bool> {
    let bytes = match frame.payload {
        Payload::Bytes(b) => Bytes::from(b),
        Payload::Owned(b) => Bytes::from(b),
        Payload::Borrowed(b) => Bytes::copy_from_slice(b),
        Payload::BorrowedMut(b) => Bytes::copy_from_slice(b),
    };

    match frame.opcode {
        OpCode::Close => Ok(false),
        OpCode::Text => {
            // Text is guaranteed to be utf-8 by fastwebsockets
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            send_to
                .send(ASGIReceiveEvent::new_websocket_receive(None, Some(text)))
                .await?;
            Ok(true)
        }
        OpCode::Binary => {
            send_to
                .send(ASGIReceiveEvent::new_websocket_receive(Some(bytes), None))
                .await?;
            Ok(true)
        }
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::{do_app_iteration, WebsocketHandler};

    use bytes::Bytes;
    use std::result::Result;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::time::error::Elapsed;

    use asgispec::prelude::*;
    use fastwebsockets::{FragmentCollector, Role, WebSocket};
    use http::Request;

    use crate::communication_mocks::{DeterministicReceiveFromApp, SendToAppCollector};
    use crate::stream_mock::MockWebsocketStream;
    use crate::ArasResult;

    fn build_websocket_request() -> Request<String> {
        Request::builder()
            .header("Connection", "Upgrade")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("sec-websocket-version", "13")
            .body("".to_string())
            .expect("Failed to build request")
    }

    fn build_websocket_client() -> (MockWebsocketStream, Arc<Mutex<FragmentCollector<MockWebsocketStream>>>) {
        let stream = MockWebsocketStream::new();
        let ws = WebSocket::after_handshake(stream.clone(), Role::Client);
        let arc_ws = Arc::new(Mutex::new(FragmentCollector::new(ws)));
        (stream, arc_ws)
    }

    #[tokio::test]
    async fn test_send_message_to_websocket_client() {
        let (stream, ws) = build_websocket_client();
        let msg: Result<ArasResult<ASGISendEvent>, Elapsed> = Ok(Ok(ASGISendEvent::new_websocket_send(
            Some(Bytes::from("hello client")),
            None,
        )));

        let result = do_app_iteration(msg, ws.clone()).await;

        assert!(result.is_ok());
        assert!(result.unwrap());

        let written = stream.written_unmasked().unwrap();

        assert!(written.len() == 1);
        assert!(written[0] == "hello client");
    }

    #[tokio::test]
    async fn test_accept_websocket_connection() {
        let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
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
        let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
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
        let handler = WebsocketHandler::new(std::time::Duration::from_secs(5));
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
