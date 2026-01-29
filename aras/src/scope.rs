use asgispec::prelude::*;
use asgispec::scope::{HTTPScope, LifespanScope, WebsocketScope};
use bytes::Bytes;
use http::Request;

use crate::types::ConnectionInfo;

/// Given a state build ASGI scopes for the supported protocols.
#[derive(Clone)]
pub(crate) struct ScopeFactory<S: State> {
    state: S,
}

impl<S: State> ScopeFactory<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }
}

impl<S: State> ScopeFactory<S> {
    pub fn build_http<B>(&self, conn: &ConnectionInfo, request: &Request<B>) -> Scope<S> {
        let scope = HTTPScope::new(
            ASGIScope::default(),
            format!("{:?}", request.version()),
            request.method().as_str().to_owned(),
            String::from("http"),
            request.uri().path().to_owned(),
            Bytes::from(request.uri().to_string()),
            Bytes::from(request.uri().query().unwrap_or("").to_owned()),
            String::from(""), // Optional, default for now
            request
                .headers()
                .into_iter()
                .map(|(name, value)| {
                    (
                        Bytes::from(name.as_str().to_string()),
                        Bytes::from(value.as_bytes().to_vec()),
                    )
                })
                .collect(),
            Some((conn.client_ip.to_owned(), conn.client_port)),
            Some((conn.server_ip.to_owned(), conn.server_port)),
            Some(self.state.clone()),
        );
        scope.into()
    }

    pub fn build_websocket<B>(&self, conn: &ConnectionInfo, request: &Request<B>) -> Scope<S> {
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
            Bytes::from(request.uri().to_string()),
            Bytes::from(request.uri().query().unwrap_or("").to_owned()),
            String::from(""), // TODO: Optional, default for now
            request
                .headers()
                .into_iter()
                .map(|(name, value)| {
                    (
                        Bytes::from(name.as_str().to_string()),
                        Bytes::from(value.as_bytes().to_vec()),
                    )
                })
                .collect(),
            Some((conn.client_ip.to_owned(), conn.client_port)),
            Some((conn.server_ip.to_owned(), conn.server_port)),
            subprotocols,
            Some(self.state.clone()),
        );
        scope.into()
    }

    pub fn build_lifespan(&self) -> Scope<S> {
        Scope::Lifespan(LifespanScope::new(ASGIScope::default(), Some(self.state.clone())))
    }
}