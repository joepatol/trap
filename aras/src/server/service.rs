use derive_more::derive::Constructor;
use http_body_util::{Full, BodyExt};
use hyper::service::Service;
use hyper::Request;
use hyper::body::Incoming;
use log::error;
use asgispec::prelude::*;
use asgispec::scope::{HTTPScope, WebsocketScope};

use crate::application::ApplicationFactory;
use crate::error::{Error, Result};
use crate::server::ConnectionInfo;
use crate::types::{Response, ServiceFuture};
use crate::http::serve_http;
use crate::websocket::serve_websocket;

#[derive(Constructor, Clone)]
pub struct ASGIService<S: State, T: ASGIApplication<S>> {
    app_factory: ApplicationFactory<S, T>,
    conn_info: ConnectionInfo,
    state: S,
}

impl<S: State + 'static, T: ASGIApplication<S> + 'static> Service<Request<Incoming>> for ASGIService<S, T> {
    type Error = Error;
    type Response = Response;
    type Future = ServiceFuture;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        // TODO: use app receiver to check output?
        let asgi_app = self.app_factory.build();
        if is_websocket_request(&req) {
            let scope = create_ws_scope(&req, &self.conn_info, self.state.clone());
            let (called_app, _) = asgi_app.call(scope);
            Box::pin(finalize(Box::pin(serve_websocket(called_app, req))))
        } else {
            let scope = create_http_scope(&req, &self.conn_info, self.state.clone());
            let (called_app, _) = asgi_app.call(scope);
            Box::pin(finalize(Box::pin(serve_http(called_app, req))))
        }
    }
}

async fn finalize(result: ServiceFuture) -> Result<Response> {
    match result.await {
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

fn is_websocket_request(value: &Request<Incoming>) -> bool {
    if let Some(header_value) = value.headers().get("upgrade") {
        if header_value == "websocket" {
            return true;
        }
    };
    false
}

fn create_ws_scope<S: State>(request: &Request<Incoming>, connection_info: &ConnectionInfo, state: S) -> Scope<S> {
    let subprotocols = 
        request
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
            .map(
                |(name, value)| {
                    (name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec())
                }
            )
            .collect(),
        Some((connection_info.client_ip.to_owned(), connection_info.client_port)),
        Some((connection_info.server_ip.to_owned(), connection_info.server_port)),
        subprotocols,
        Some(state),
    );
    Scope::Websocket(scope)
}

fn create_http_scope<S: State>(request: &Request<Incoming>, connection_info: &ConnectionInfo, state: S) -> Scope<S> {
    let scope = HTTPScope::new(
        ASGIScope::default(),
        format!("{:?}", request.version()),
        request.method().as_str().to_owned(),
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
        Some(state),
    );
    Scope::HTTP(scope)
}