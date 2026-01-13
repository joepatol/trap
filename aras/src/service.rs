use std::fmt::Display;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use asgispec::prelude::*;
use fastwebsockets::upgrade::is_upgrade_request;
use http::Request;
use http_body::Body;
use http_body_util::{BodyExt, Full};
use log::error;
use std::future::Future;
use tower::Service;

use crate::communication::CommunicationFactory;
use crate::protocols::{HTTPHandler, WebsocketHandler};
use crate::scope::ScopeFactory;
use crate::types::{ConnectionInfo, Response, ServiceFuture};
use crate::{ArasError, ArasResult};

#[derive(Clone)]
pub struct ArasASGIService<A: ASGIApplication> {
    scope_factory: ScopeFactory<A::State>,
    communication_factory: CommunicationFactory<A>,
    connection: ConnectionInfo,
    asgi_timeout_secs: u64,
}

impl<A: ASGIApplication> ArasASGIService<A> {
    pub fn new(application: A, state: A::State, connection: ConnectionInfo, asgi_timeout_secs: u64) -> Self {
        let scope_factory = ScopeFactory::new(state);
        let communication_factory = CommunicationFactory::new(application);
        Self {
            scope_factory,
            communication_factory,
            connection,
            asgi_timeout_secs,
        }
    }

    pub(crate) fn new_from_factories(
        scope_factory: ScopeFactory<A::State>,
        communication_factory: CommunicationFactory<A>,
        connection: ConnectionInfo,
        asgi_timeout_secs: u64,
    ) -> Self {
        Self {
            scope_factory,
            communication_factory,
            connection,
            asgi_timeout_secs,
        }
    }
}

impl<A, B> Service<Request<B>> for ArasASGIService<A>
where
    A: ASGIApplication + 'static,
    B: Body + Send + 'static,
    <B as Body>::Error: Display + Send,
    <B as Body>::Data: Send,
{
    type Error = ArasError;
    type Response = Response;
    type Future = ServiceFuture;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let timeout = Duration::from_secs(self.asgi_timeout_secs);
        if is_upgrade_request(&request) {
            let scope = self.scope_factory.build_websocket(&self.connection, &request);
            let (send_to_app, receive_from_app) = self.communication_factory.build(scope);
            let handler = WebsocketHandler::new(timeout, 1000, 100); // TIODO: make these configurable
            let fut = handler.handle(send_to_app, receive_from_app, request);
            Box::pin(handle_error(fut))
        } else {
            let scope = self.scope_factory.build_http(&self.connection, &request);
            let (send_to_app, receive_from_app) = self.communication_factory.build(scope);
            let handler = HTTPHandler::new(timeout);
            let fut = handler.handle(send_to_app, receive_from_app, request);
            Box::pin(handle_error(fut))
        }
    }
}

async fn handle_error(fut: impl Future<Output = ArasResult<Response>>) -> ArasResult<Response> {
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
            Ok(response.map_err(|e| Arc::new(e))?)
        }
    }
}
