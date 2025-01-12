use std::sync::Arc;
use std::fmt::Debug;

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
use asgispec::prelude::*;
use asgispec::scope::WebsocketScope;

use crate::application::{ApplicationWrapper, RunningApplication};
use crate::error::Result;
use crate::types::{Response, ConnectionInfo};
use crate::error::{Error, UnexpectedShutdownSrc as SRC};

#[derive(Constructor)]
pub(crate) struct WebsocketHandler;

impl WebsocketHandler {
    pub async fn serve<A, B>(self, application: A, mut request: Request<B>, conn: ConnectionInfo, state: A::State) -> Result<Response> 
    where
        A: ASGIApplication + 'static,
        B: Body + Send + 'static,
        <B as hyper::body::Body>::Error: Debug,
    {
        let scope: Scope<A::State> = create_ws_scope(&request, &conn, state);
        let mut called_app = ApplicationWrapper::from(&application).call(scope);

        let (accepted, app_response) = accept_websocket_connection(&mut called_app).await?;
    
        if accepted {
            let (upgrade_response, fut) = upgrade::upgrade(&mut request)?;
            tokio::task::spawn(async move {
                match run_accepted_websocket(&mut called_app, fut).await {
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

async fn accept_websocket_connection(called_app: &mut RunningApplication) -> Result<(bool, Response)> {
    let mut builder = hyper::Response::builder();
    called_app.send_to(ASGIReceiveEvent::new_websocket_connect()).await?;

    match called_app.receive_from().await {
        Some(ASGISendEvent::WebsocketAccept(msg)) => {
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
        Some(ASGISendEvent::WebsocketClose(msg)) => {
            let body = Full::new(msg.reason.into()).map_err(|never| match never {}).boxed();
            builder = builder.status(StatusCode::FORBIDDEN);
            Ok((false, builder.body(body)?))
        }
        None => Err(Error::unexpected_shutdown(
            SRC::Application,
            "shutdown during websocket handshake".into(),
        )),
        Some(msg) => Err(Error::unexpected_asgi_message(Box::new(msg))),
    }
}

enum WsIteration<'a> {
    ReceiveClient(std::result::Result<fastwebsockets::Frame<'a>, fastwebsockets::WebSocketError>),
    ReceiveApplication(Option<ASGISendEvent>),
}

async fn run_accepted_websocket(called_app: &mut RunningApplication, upgraded_io: UpgradeFut) -> Result<()> {
    let ws = Arc::new(Mutex::new(FragmentCollector::new(upgraded_io.await?)));

    loop {
        let ws_iter = ws.clone();
        let mut ws_locked = ws_iter.lock().await;

        let iteration: WsIteration<'_> = tokio::select! {
            out = ws_locked.read_frame() => WsIteration::ReceiveClient(out),
            out = called_app.receive_from() => WsIteration::ReceiveApplication(out),
        };

        drop(ws_locked); // Drop the lock so it can be acquired for writing

        match iteration {
            WsIteration::ReceiveClient(frame) => {
                if !do_server_iteration(frame?, called_app).await? {
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

    called_app
        .send_to(ASGIReceiveEvent::new_websocket_disconnect(1005))
        .await?;
    called_app.close().await;

    Ok(())
}

async fn do_app_iteration(
    msg: Option<ASGISendEvent>,
    ws: Arc<Mutex<FragmentCollector<TokioIo<Upgraded>>>>,
) -> Result<bool> {
    match msg {
        Some(ASGISendEvent::WebsocketSend(msg)) => {
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
        Some(ASGISendEvent::WebsocketClose(msg)) => {
            let payload = Payload::Owned(msg.reason.into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Ok(false)
        }
        None => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(Error::unexpected_shutdown(SRC::Application, "shutdown while open websocket connection".into()))
        }
        Some(msg) => {
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(Error::unexpected_asgi_message(Box::new(format!("Received invalid ASGI message in websocket loop. Got: {msg:?}"))))
        }
    }
}

async fn do_server_iteration(frame: Frame<'_>, asgi_app: &mut RunningApplication) -> Result<bool> {
    let frame_bytes = frame.payload.to_vec();

    match frame.opcode {
        OpCode::Close => {
            Ok(false)
        },
        OpCode::Text => {
            // Text is guaranteed to be utf-8 by fastwebsockets
            let text = String::from_utf8(frame_bytes).unwrap();
            asgi_app.send_to(ASGIReceiveEvent::new_websocket_receive(None, Some(text))).await?;
            Ok(true)
        }
        OpCode::Binary => {
            asgi_app.send_to(ASGIReceiveEvent::new_websocket_receive(Some(frame_bytes), None)).await?;
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
            // TODO: is default here desirable?
            let mut txt = String::from_utf8(v.as_bytes().to_vec()).unwrap_or("".to_string());
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
        String::from(""), // Optional, default for now
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