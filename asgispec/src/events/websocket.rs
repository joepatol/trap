#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsocketConnectEvent {
    pub type_: String,
}

impl WebsocketConnectEvent {
    pub fn new() -> Self {
        Self { type_: "websocket.connect".into() }
    }
}

impl std::fmt::Display for WebsocketConnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for WebsocketAcceptEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        if self.subprotocol.is_some() {
            writeln!(f, "subprotocol: {}", self.subprotocol.clone().unwrap())?;
        } else {
            writeln!(f, "subprotocol: None")?;
        }
        writeln!(f, "headers:")?;
        for (name, value) in &self.headers {
            writeln!(f, "  {}: {}", String::from_utf8_lossy(name), String::from_utf8_lossy(value))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for WebsocketReceiveEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        if self.bytes.is_some() {
            writeln!(f, "bytes: {}", String::from_utf8_lossy(&self.bytes.clone().unwrap()))?;
        } else {
            writeln!(f, "bytes: None")?;
        }
        if self.text.is_some() {
            writeln!(f, "text: {}", self.text.clone().unwrap())?;
        } else {
            writeln!(f, "text: None")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for WebsocketSendEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        if self.bytes.is_some() {
            writeln!(f, "bytes: {}", String::from_utf8_lossy(&self.bytes.clone().unwrap()))?;
        } else {
            writeln!(f, "bytes: None")?;
        }
        if self.text.is_some() {
            writeln!(f, "text: {}", self.text.clone().unwrap())?;
        } else {
            writeln!(f, "text: None")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for WebsocketDisconnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "code: {}", self.code)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for WebsocketCloseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "code: {}", self.code)?;
        writeln!(f, "reason: {}", self.reason)?;
        Ok(())
    }
}