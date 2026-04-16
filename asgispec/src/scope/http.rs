use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::spec::{ASGIScope, DisplaySerde, State};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HTTPScope<S: State> {
    pub asgi: ASGIScope,
    pub http_version: String,
    pub method: String,
    pub scheme: String,
    pub path: String,
    pub raw_path: Bytes,
    pub query_string: Bytes,
    pub root_path: String,
    pub headers: Vec<(Bytes, Bytes)>,
    pub client: Option<(String, u16)>,
    pub server: Option<(String, u16)>,
    pub state: Option<S>,
}

impl<S: State> HTTPScope<S> {
    pub fn new(
        asgi: ASGIScope,
        http_version: String,
        method: String,
        scheme: String,
        path: String,
        raw_path: Bytes,
        query_string: Bytes,
        root_path: String,
        headers: Vec<(Bytes, Bytes)>,
        client: Option<(String, u16)>,
        server: Option<(String, u16)>,
        state: Option<S>,
    ) -> Self {
        Self {
            asgi,
            http_version,
            method,
            scheme,
            path,
            raw_path,
            query_string,
            root_path,
            headers,
            client,
            server,
            state,
        }
    }
}

impl<S: State + Serialize> std::fmt::Display for HTTPScope<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}
