#[derive(Debug, Clone)]
pub struct WebsocketConnectEvent {
    pub type_: String,
}

impl WebsocketConnectEvent {
    pub fn new() -> Self {
        Self { type_: "websocket.connect".into() }
    }
}

#[derive(Debug, Clone)]
pub struct WebsocketAcceptEvent {
    pub type_: String,
    pub subprotocol: Option<String>,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

impl WebsocketAcceptEvent {
    pub fn new(
        subprotocol: Option<String>,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> Self {
        Self { type_:  "websocket.accept".into(), subprotocol, headers }
    }
}

#[derive(Debug, Clone)]
pub struct WebsocketReceiveEvent {
    pub type_: String,
    pub bytes: Option<Vec<u8>>,
    pub text: Option<String>,
}

impl WebsocketReceiveEvent {
    pub fn new(
        bytes: Option<Vec<u8>>,
        text: Option<String>,
    ) -> Self {
        // TODO: at least one of bytes or text should be present
        Self { type_: "websocket.receive".into(), bytes, text }
    } 
}

#[derive(Debug, Clone)]
pub struct WebsocketSendEvent {
    pub type_: String,
    pub bytes: Option<Vec<u8>>,
    pub text: Option<String>,
}

impl WebsocketSendEvent {
    pub fn new(
        bytes: Option<Vec<u8>>,
        text: Option<String>,
    ) -> Self {
        // TODO: at least one of bytes or text should be present
        Self { type_: "websocket.send".into(), bytes, text }
    } 
}

#[derive(Debug, Clone)]
pub struct WebsocketDisconnectEvent {
    pub type_: String,
    pub code: usize,
}

impl WebsocketDisconnectEvent {
    pub fn new(code: usize) -> Self {
        Self { type_: "websocket.disconnect".into(), code }
    }
}

impl Default for WebsocketDisconnectEvent {
    fn default() -> Self {
        Self { type_: "websocket.disconnect".into(), code: 1005 }
    }
}

#[derive(Debug, Clone)]
pub struct WebsocketCloseEvent {
    pub type_: String,
    pub code: usize,
    pub reason: String,
}

impl WebsocketCloseEvent {
    pub fn new(code: Option<usize>, reason: String) -> Self {
        Self { type_: "websocket.close".into(), code: code.unwrap_or(1000), reason }
    }
}