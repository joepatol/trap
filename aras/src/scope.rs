use http::Request;
use asgispec::prelude::*;
use asgispec::scope::{HTTPScope, WebsocketScope, LifespanScope}; 
use crate::types::ConnectionInfo;

/// Given a state build ASGI scopes for the supported protocols.
#[derive(Clone)]
pub(crate) struct ScopeFactory<A: ASGIApplication> {
    state: A::State,
}

impl<A: ASGIApplication> ScopeFactory<A> {
    pub fn new(state: A::State) -> Self {
        Self { state }
    }
}

impl<A: ASGIApplication> ScopeFactory<A> {
    pub fn build_http<B>(&self, conn: &ConnectionInfo, request: &Request<B>) -> Scope<A::State> {
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
            Some((conn.client_ip.to_owned(), conn.client_port)),
            Some((conn.server_ip.to_owned(), conn.server_port)),
            Some(self.state.clone()),
        );
        scope.into()
    }

    pub fn build_websocket<B>(&self, conn: &ConnectionInfo, request: &Request<B>) -> Scope<A::State> {
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
            Some((conn.client_ip.to_owned(), conn.client_port)),
            Some((conn.server_ip.to_owned(), conn.server_port)),
            subprotocols,
            Some(self.state.clone()),
        );
        scope.into()
    }

    pub fn build_lifespan(&self) -> Scope<A::State> {
        Scope::Lifespan(LifespanScope::new(ASGIScope::default(), Some(self.state.clone())))
    }
}