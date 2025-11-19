use std::fmt::Debug;

use asgispec::prelude::*;
use derive_more::derive::Constructor;
use http_body::Body;
use http::Request;
use http_body_util::{BodyExt, Full};
use log::error;
use tower::Service;
use std::future::Future;

use crate::error::{Result, Error};
use crate::protocols::{HTTPHandler, WebsocketHandler};
use crate::types::{ServiceFuture, ConnectionInfo, Response};

// A Tower service that handles an ASGI application
#[derive(Constructor, Clone)]
pub struct ArasASGIService<A: ASGIApplication> {
    application: A,
    state: A::State,
    conn: ConnectionInfo,
}

impl<A, B> Service<Request<B>> for ArasASGIService<A> 
where
    A: ASGIApplication + 'static,
    B: Body + Send + 'static,
    <B as Body>::Error: Debug + Send,
    <B as Body>::Data: Send,
{
    type Error = Error;
    type Response = Response;
    type Future = ServiceFuture;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        if is_websocket_request(&request) {
            let handler = WebsocketHandler::new();
            let fut = handler.serve(
                self.application.clone(), 
                request,
                self.conn.clone(),
                self.state.clone(),
            );
            Box::pin(handle_error(fut))
        } else {
            let handler = HTTPHandler::new();
            let fut = handler.serve(
                self.application.clone(), 
                request,
                self.conn.clone(),
                self.state.clone(),
            );
            Box::pin(handle_error(fut))
        }
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

fn is_websocket_request<B>(value: &Request<B>) -> bool {
    if let Some(header_value) = value.headers().get("upgrade") {
        if header_value == "websocket" {
            return true;
        }
    };
    false
}