use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use asgispec::prelude::*;
use bytes::Bytes;
use bytes::BytesMut;
use derive_more::derive::Constructor;
use fastwebsockets::upgrade::UpgradeFut;
use fastwebsockets::{upgrade, FragmentCollector, Frame, OpCode, Payload};
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper::body::Body;
use hyper::upgrade::Upgraded;
use hyper::Request;
use hyper_util::rt::TokioIo;
use log::error;
use tokio::sync::Mutex;

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::errors::Error;
use crate::errors::Result;
use crate::types::Response;

#[derive(Constructor)]
pub(crate) struct WebsocketHandler {
    timeout_secs: u64,
}

impl WebsocketHandler {
    pub async fn handle<B>(
        self,
        mut send_to_app: impl SendToASGIApp + 'static,
        mut receive_from_app: impl ReceiveFromASGIApp + 'static,
        mut request: Request<B>,
    ) -> Result<Response>
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
            if let Err(e) = self
                .run_accepted_websocket(&mut send_to_app, &mut receive_from_app, fut)
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
    ) -> Result<(bool, Response)> {
        let dur = Duration::from_secs(self.timeout_secs);
        let mut builder = hyper::Response::builder();
        send_to.send(ASGIReceiveEvent::new_websocket_connect()).await?;

        match tokio::time::timeout(dur, receive_from.receive()).await {
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
            Ok(Ok(msg)) => Err(Error::unexpected_asgi_message(Arc::new(msg))),
            Err(e) => Err(e.into()),
        }
    }

    async fn run_accepted_websocket(
        &self,
        send_to: &mut impl SendToASGIApp,
        receive_from: &mut impl ReceiveFromASGIApp,
        upgraded_io: UpgradeFut,
    ) -> Result<()> {
        let dur = Duration::from_secs(self.timeout_secs);
        let ws = Arc::new(Mutex::new(FragmentCollector::new(upgraded_io.await?)));

        loop {
            let ws_iter = ws.clone();
            let mut ws_locked = ws_iter.lock().await;

            let iteration: WsIteration<'_> = tokio::select! {
                out = ws_locked.read_frame() => WsIteration::ReceiveClient(out),
                out = tokio::time::timeout(dur, receive_from.receive()) => WsIteration::ReceiveApplication(out),
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

fn merge_responses(
    app_response: Response,
    upgrade_response: http::Response<http_body_util::Empty<Bytes>>,
) -> Result<Response> {
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
    ReceiveClient(std::result::Result<fastwebsockets::Frame<'a>, fastwebsockets::WebSocketError>),
    ReceiveApplication(std::result::Result<Result<ASGISendEvent>, tokio::time::error::Elapsed>),
}

async fn do_app_iteration(
    msg: std::result::Result<Result<ASGISendEvent>, tokio::time::error::Elapsed>,
    ws: Arc<Mutex<FragmentCollector<TokioIo<Upgraded>>>>,
) -> Result<bool> {
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
            Err(Error::unexpected_asgi_message(Arc::new(format!(
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

async fn do_server_iteration(frame: Frame<'_>, send_to: &mut impl SendToASGIApp) -> Result<bool> {
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
