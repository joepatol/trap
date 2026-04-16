use std::fmt::Display;
use std::task::{Context, Poll};
use std::time::Duration;

use asgispec::prelude::*;
use fastwebsockets::upgrade::is_upgrade_request;
use http::Request;
use http_body::Body;
use tower::Service;

use crate::communication::CommunicationFactory;
use crate::protocols::{HTTPHandler, WebsocketHandler};
use crate::scope::ScopeFactory;
use crate::types::{ConnectionInfo, Response, ServiceFuture};
use crate::ArasError;

#[derive(Clone)]
pub struct ArasASGIService<A: ASGIApplication> {
    scope_factory: ScopeFactory<A::State>,
    communication_factory: CommunicationFactory<A>,
    connection: ConnectionInfo,
    asgi_timeout: Duration,
    max_ws_frame_size: usize,
}

impl<A: ASGIApplication> ArasASGIService<A> {
    pub fn new(
        application: A,
        state: A::State,
        connection: ConnectionInfo,
        asgi_timeout: Duration,
        communication_queue_size: usize,
        max_ws_frame_size: usize,
    ) -> Self {
        let scope_factory = ScopeFactory::new(state);
        let communication_factory = CommunicationFactory::new(application, communication_queue_size);
        Self {
            scope_factory,
            communication_factory,
            connection,
            asgi_timeout,
            max_ws_frame_size,
        }
    }

    pub(crate) fn new_from_factories(
        scope_factory: ScopeFactory<A::State>,
        communication_factory: CommunicationFactory<A>,
        connection: ConnectionInfo,
        asgi_timeout: Duration,
        max_ws_frame_size: usize,
    ) -> Self {
        Self {
            scope_factory,
            communication_factory,
            connection,
            asgi_timeout,
            max_ws_frame_size,
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
        if is_upgrade_request(&request) {
            let scope = self
                .scope_factory
                .build_websocket(&self.connection, &request);
            let (send_to_app, receive_from_app) = self.communication_factory.build(scope);
            let handler = WebsocketHandler::new(
                self.asgi_timeout,
                self.max_ws_frame_size,
                format!(
                    "{}:{}",
                    &self.connection.client_ip, self.connection.client_port
                ),
            );
            Box::pin(handler.handle(send_to_app, receive_from_app, request))
        } else {
            let scope = self.scope_factory.build_http(&self.connection, &request);
            let (send_to_app, receive_from_app) = self.communication_factory.build(scope);
            let handler = HTTPHandler::new(self.asgi_timeout);
            Box::pin(handler.handle(send_to_app, receive_from_app, request))
        }
    }
}
