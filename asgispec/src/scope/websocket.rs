use crate::spec::{ASGIScope, State};

#[derive(Debug, Clone)]
pub struct WebsocketScope<S: State> {
    pub type_: String,
    pub asgi: ASGIScope,
    pub http_version: String,
    pub scheme: String,
    pub path: String,
    pub raw_path: Vec<u8>,
    pub query_string: Vec<u8>,
    pub root_path: String,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
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
        raw_path: Vec<u8>,
        query_string: Vec<u8>,
        root_path: String,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
        client: Option<(String, u16)>,
        server: Option<(String, u16)>,
        subprotocols: Vec<String>,
        state: Option<S>,
    ) -> Self {
        Self {
            type_: String::from("websocket"),
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

impl<S: State> std::fmt::Display for WebsocketScope<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "asgi: {}", self.asgi)?;
        writeln!(f, "http_version: {}", self.http_version)?;
        writeln!(f, "scheme: {}", self.scheme)?;
        writeln!(f, "path: {}", self.path)?;
        writeln!(f, "raw_path: {}", String::from_utf8_lossy(&self.raw_path))?;
        writeln!(f, "query_string: {}", String::from_utf8_lossy(&self.query_string))?;
        writeln!(f, "root_path: {}", self.root_path)?;

        writeln!(f, "headers:")?;
        for (name, value) in &self.headers {
            writeln!(f, "  {}: {}", String::from_utf8_lossy(name), String::from_utf8_lossy(value))?;
        }

        if let Some((ip, port)) = &self.client {
            writeln!(f, "client: {}:{}", ip, port)?;
        } else {
            writeln!(f, "client: None")?;
        }

        if let Some((ip, port)) = &self.server {
            writeln!(f, "server: {}:{}", ip, port)?;
        } else {
            writeln!(f, "server: None")?;
        }

        writeln!(f, "subprotocols:")?;
        for subprotocol in self.subprotocols.iter() {
            writeln!(f, "  {subprotocol}")?;
        }

        if self.state.is_some() {
            writeln!(f, "state: {}", self.state.clone().unwrap())?;
        } else {
            writeln!(f, "state: None")?;
        }

        Ok(())
    }
}
