use std::sync::Arc;

use bytes::Bytes;
use bytes::BytesMut;
use fastwebsockets::upgrade::UpgradeFut;
use fastwebsockets::{upgrade, FragmentCollector, Frame, OpCode, Payload};
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::upgrade::Upgraded;
use hyper::Request;
use hyper_util::rt::TokioIo;
use log::error;
use tokio::sync::Mutex;

use crate::application::RunningApplication;
use crate::asgispec::{ASGIReceiveEvent, ASGISendEvent};
use crate::error::Result;
use crate::types::Response;
use crate::Error;

pub async fn serve_websocket(called_app: RunningApplication, mut req: Request<Incoming>) -> Result<Response> {
    let (accepted, app_response) = accept_websocket_connection(called_app.clone()).await?;

    if accepted {
        let (upgrade_response, fut) = upgrade::upgrade(&mut req)?;
        tokio::task::spawn(async move {
            match run_accepted_websocket(called_app, fut).await {
                Ok(_) => (),
                Err(e) => error!("Error while serving websocket; {e}"),
            }
        });
        // The application might have send a body and additional headers.
        // If connection is accepted, merge the application response with hyper/fastwebsockets
        // proposed response. This way we can make use of their upgrade functionality
        // while maintaining required control by the application
        return Ok(merge_responses(app_response, upgrade_response)?);
    };
    Ok(app_response)
}

async fn accept_websocket_connection(mut called_app: RunningApplication) -> Result<(bool, Response)> {
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
            "Application",
            "shutdown during websocket handshake".into(),
        )),
        Some(msg) => Err(Error::unexpected_asgi_message(Box::new(msg))),
    }
}

enum WsIteration<'a> {
    ReceiveClient(std::result::Result<fastwebsockets::Frame<'a>, fastwebsockets::WebSocketError>),
    ReceiveApplication(Option<ASGISendEvent>),
}

async fn run_accepted_websocket(mut called_app: RunningApplication, upgraded_io: UpgradeFut) -> Result<()> {
    let ws = Arc::new(Mutex::new(FragmentCollector::new(upgraded_io.await?)));

    loop {
        println!("new ws loop");
        let mut app_iter = called_app.clone();
        let ws_iter = ws.clone();
        println!("acquiring lock");
        let mut ws_locked = ws_iter.lock().await;

        println!("waiting for iteration");
        let iteration: WsIteration<'_> = tokio::select! {
            out = ws_locked.read_frame() => WsIteration::ReceiveClient(out),
            out = app_iter.receive_from() => WsIteration::ReceiveApplication(out),
        };

        println!("drop lock");
        drop(ws_locked); // Drop the lock so it can be acquired for writing

        println!("execute iteration");
        match iteration {
            WsIteration::ReceiveClient(frame) => {
                println!("server iteration");
                let app_clone = called_app.clone();
                if let false = do_server_iteration(frame?, app_clone).await? {
                    break;
                };
            }
            WsIteration::ReceiveApplication(msg) => {
                println!("app iteration");
                let ws_clone = ws.clone();
                if let false = do_app_iteration(msg, ws_clone).await? {
                    break;
                };
            }
        };
    }

    println!("break ws loop, disconnecting");
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
                println!("Received app data: {data}");
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
            println!("Received ws close from app");
            let payload = Payload::Owned(msg.reason.into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Ok(false)
        }
        None => {
            println!("Received None from app");
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(Error::unexpected_shutdown("Application", "shutdown while open websocket connection".into()))
        }
        Some(msg) => {
            println!("Received invalid msg from app {msg:?}");
            let payload = Payload::Owned(String::from("Internal server error").into_bytes());
            let frame = Frame::new(true, OpCode::Close, None, payload);
            ws.lock().await.write_frame(frame).await?;
            Err(Error::unexpected_asgi_message(Box::new(format!("Received invalid ASGI message in websocket loop. Got: {msg:?}"))))
        }
    }
}

async fn do_server_iteration(frame: Frame<'_>, asgi_app: RunningApplication) -> Result<bool> {
    let frame_bytes = frame.payload.to_vec();

    match frame.opcode {
        OpCode::Close => {
            println!("Received close from client");
            Ok(false)
        },
        OpCode::Text => {
            println!("Received text from client");
            // Text is guaranteed to be utf-8 by fastwebsockets
            let text = String::from_utf8(frame_bytes).unwrap();
            println!("{text}");
            match asgi_app
                .send_to(ASGIReceiveEvent::new_websocket_receive(None, Some(text)))
                .await {
                    Ok(()) => (),
                    Err(e) => {
                        println!("{e}");
                        return Err(e)
                    }
                }
            Ok(true)
        }
        OpCode::Binary => {
            println!("Received binary from client");
            asgi_app
                .send_to(ASGIReceiveEvent::new_websocket_receive(Some(frame_bytes), None))
                .await?;
            Ok(true)
        }
        _ => {
            println!("Received other msg from client");
            Ok(true)
        },
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
