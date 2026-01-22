use serde::{Deserialize, Serialize};
use bytes::Bytes;

use crate::spec::{ASGIDisplay, ASGIScope, State};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketScope<S: State> {
    pub asgi: ASGIScope,
    pub http_version: String,
    pub scheme: String,
    pub path: String,
    pub raw_path: Bytes,
    pub query_string: Bytes,
    pub root_path: String,
    pub headers: Vec<(Bytes, Bytes)>,
    pub client: Option<(String, u16)>,
    pub server: Option<(String, u16)>,
    pub subprotocols: Vec<String>,
    pub state: Option<S>,
}

impl<S: State> WebsocketScope<S> {
    pub fn new(
        asgi: ASGIScope,
        http_version: String,
        scheme: String,
        path: String,
        raw_path: Bytes,
        query_string: Bytes,
        root_path: String,
        headers: Vec<(Bytes, Bytes)>,
        client: Option<(String, u16)>,
        server: Option<(String, u16)>,
        subprotocols: Vec<String>,
        state: Option<S>,
    ) -> Self {
        Self {
            asgi,
            http_version,
            scheme,
            path,
            raw_path,
            query_string,
            root_path,
            headers,
            client,
            server,
            subprotocols,
            state,
        }
    }
}

impl<S: State + Serialize> std::fmt::Display for WebsocketScope<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}
