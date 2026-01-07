use std::fmt::Display;
use std::sync::Arc;

use asgispec::prelude::*;
use asgispec::scope::WebsocketScope;
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

use crate::communication::{CommunicatorFactory, SendToApp, ReceiveFromApp};
use crate::errors::Result;
use crate::errors::Error;
use crate::types::{ConnectionInfo, Response};

#[derive(Constructor)]
pub(crate) struct WebsocketHandler;

impl WebsocketHandler {
    pub async fn serve<A, B>(
        self,
        application: A,
        mut request: Request<B>,
        conn: ConnectionInfo,
        state: A::State,
    ) -> Result<Response>
    where
        A: ASGIApplication + 'static,
        B: Body + Send + 'static,
        B::Error: Display,
    {
        let factory = CommunicatorFactory::new();
        let scope: Scope<A::State> = create_ws_scope(&request, &conn, state);
        let (mut send_to, mut receive_from) = factory.build(application, scope);

        let (accepted, app_response) = accept_websocket_connection(&mut send_to, &mut receive_from).await?;

        if accepted {
            let (upgrade_response, fut) = upgrade::upgrade(&mut request)?;
            tokio::task::spawn(async move {
                match run_accepted_websocket(&mut send_to, &mut receive_from, fut).await {
                    Ok(_) => (),
                    Err(e) => error!("Error while serving websocket; {e}"),
                }
            });
            // The application might have send a body and additional headers.
            // If connection is accepted, merge the application response with hyper/fastwebsockets
            // proposed response. This way we can make use of their upgrade functionality
            // while maintaining required control by the application
            return merge_responses(app_response, upgrade_response);
        };
        Ok(app_response)
    }
}

async fn accept_websocket_connection(send_to: &mut SendToApp, receive_from: &mut ReceiveFromApp) -> Result<(bool, Response)> {
    let mut builder = hyper::Response::builder();
    send_to.push(ASGIReceiveEvent::new_websocket_connect()).await?;

    match receive_from.next().await {
        Ok(ASGISendEvent::WebsocketAccept(msg)) => {
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
        Ok(ASGISendEvent::WebsocketClose(msg)) => {
            let body = Full::new(msg.reason.into()).map_err(|never| match never {}).boxed();
            builder = builder.status(StatusCode::FORBIDDEN);
            Ok((false, builder.body(body)?))
        }
        Err(e) => Err(e),
        Ok(msg) => Err(Error::unexpected_asgi_message(Arc::new(msg))),
    }
}

enum WsIteration<'a> {
    ReceiveClient(std::result::Result<fastwebsockets::Frame<'a>, fastwebsockets::WebSocketError>),
    ReceiveApplication(Result<ASGISendEvent>),
}

async fn run_accepted_websocket(send_to: &mut SendToApp, receive_from: &mut ReceiveFromApp, upgraded_io: UpgradeFut) -> Result<()> {
    let ws = Arc::new(Mutex::new(FragmentCollector::new(upgraded_io.await?)));

    loop {
        let ws_iter = ws.clone();
        let mut ws_locked = ws_iter.lock().await;

        let iteration: WsIteration<'_> = tokio::select! {
            out = ws_locked.read_frame() => WsIteration::ReceiveClient(out),
            out = receive_from.next() => WsIteration::ReceiveApplication(out),
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

    send_to
        .push(ASGIReceiveEvent::new_websocket_disconnect(1005))
        .await?;
    
    Ok(())
}

async fn do_app_iteration(
    msg: Result<ASGISendEvent>,
    ws: Arc<Mutex<FragmentCollector<TokioIo<Upgraded>>>>,
) -> Result<bool> {
    match msg {
        Ok(ASGISendEvent::WebsocketSend(msg)) => {
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
        Ok(ASGISendEvent::WebsocketClose(msg)) => {
            let payload = Payload::Owned(msg.reason.into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Ok(false)
        }
        Err(e) => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(e)
        }
        Ok(msg) => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(Error::unexpected_asgi_message(Arc::new(format!(
                "Received invalid ASGI message in websocket loop. Got: {msg:?}"
            ))))
        }
    }
}

async fn do_server_iteration(frame: Frame<'_>, send_to: &mut SendToApp) -> Result<bool> {
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
                .push(ASGIReceiveEvent::new_websocket_receive(None, Some(text)))
                .await?;
            Ok(true)
        }
        OpCode::Binary => {
            send_to
                .push(ASGIReceiveEvent::new_websocket_receive(Some(bytes), None))
                .await?;
            Ok(true)
        }
        _ => Ok(true),
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

fn create_ws_scope<B: Body, S: State>(request: &Request<B>, connection_info: &ConnectionInfo, state: S) -> Scope<S> {
    let subprotocols = request
        .headers()
        .into_iter()
        .filter(|(k, _)| k.as_str().to_lowercase() == "sec-websocket-protocol")
        .map(|(_, v)| {
            let mut txt = String::from_utf8_lossy(&v.as_bytes().to_vec()).to_string();
            txt.retain(|c| !c.is_whitespace());
            txt
        })
        .map(|s| s.split(",").map(|substr| substr.to_owned()).collect::<Vec<String>>())
        .flatten()
        .collect();

    let scope = WebsocketScope::new(
        ASGIScope::default(),
        format!("{:?}", request.version()),
        String::from("http"),
        request.uri().path().to_owned(),
        request.uri().to_string().as_bytes().to_vec(),
        request.uri().query().unwrap_or("").as_bytes().to_vec(),
        String::from(""), // TODO: Optional, default for now
        request
            .headers()
            .into_iter()
            .map(|(name, value)| (name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect(),
        Some((connection_info.client_ip.to_owned(), connection_info.client_port)),
        Some((connection_info.server_ip.to_owned(), connection_info.server_port)),
        subprotocols,
        Some(state),
    );
    scope.into()
}
