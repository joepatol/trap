use asgispec::prelude::*;
use http_body_util::{BodyExt, Full};
use log::error;
use std::future::Future;

use crate::error::Result;
use crate::protocols::{HTTPHandler, WebsocketHandler};
use crate::types::*;
 
pub(crate) async fn aras_asgi_service<A: ASGIApplication + 'static>(
    request: Request,
    application: A,
    conn_info: ConnectionInfo,
    state: A::State,
) -> Result<Response> {
    if is_websocket_request(&request) {
        let handler = WebsocketHandler::new();
        let fut = handler.serve(
            application.clone(), 
            request,
            conn_info.clone(),
            state.clone(),
        );
        handle_error(fut).await
    } else {
        let handler = HTTPHandler::new();
        let fut = handler.serve(
            application.clone(), 
            request,
            conn_info.clone(),
            state.clone(),
        );
        handle_error(fut).await
    }
}

async fn handle_error(fut: impl Future<Output = Result<Response>>) -> Result<Response> {
    match fut.await {
        Ok(response) => Ok(response),
        Err(error) => {
            error!("Error serving request: {error}");
            let body_text = "Internal Server Error";
            let body = Full::new(body_text.as_bytes().to_vec().into())
                .map_err(|never| match never {})
                .boxed();
            let response = hyper::Response::builder()
                .status(500)
                .header(hyper::header::CONTENT_LENGTH, body_text.len())
                .header(hyper::header::CONTENT_TYPE, "text/plain")
                .body(body);
            Ok(response?)
        }
    }
}

fn is_websocket_request(value: &Request) -> bool {
    if let Some(header_value) = value.headers().get("upgrade") {
        if header_value == "websocket" {
            return true;
        }
    };
    false
}