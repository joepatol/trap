use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsocketConnectEvent;

impl WebsocketConnectEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for WebsocketConnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: websocket.connect")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsocketAcceptEvent {
    pub subprotocol: Option<String>,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

impl WebsocketAcceptEvent {
    pub fn new(
        subprotocol: Option<String>,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> Self {
        Self { subprotocol, headers }
    }
}

impl std::fmt::Display for WebsocketAcceptEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: websocket.accept")?;
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
    pub bytes: Option<Bytes>,
    pub text: Option<String>,
}

impl WebsocketReceiveEvent {
    pub fn new(
        bytes: Option<Bytes>,
        text: Option<String>,
    ) -> Self {
        Self { bytes, text }
    } 
}

impl std::fmt::Display for WebsocketReceiveEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: websocket.receive")?;
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
    pub bytes: Option<Bytes>,
    pub text: Option<String>,
}

impl WebsocketSendEvent {
    pub fn new(
        bytes: Option<Bytes>,
        text: Option<String>,
    ) -> Self {
        Self { bytes, text }
    } 
}

impl std::fmt::Display for WebsocketSendEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: websocket.send")?;
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
    pub code: u16,
    pub reason: String,
}

impl WebsocketDisconnectEvent {
    pub fn new(code: u16, reason: String) -> Self {
        Self { code, reason }
    }
}

impl std::fmt::Display for WebsocketDisconnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: websocket.disconnect")?;
        writeln!(f, "code: {}", self.code)?;
        writeln!(f, "reason: {}", self.reason)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsocketCloseEvent {
    pub code: u16,
    pub reason: String,
}

impl WebsocketCloseEvent {
    pub fn new(code: Option<u16>, reason: String) -> Self {
        Self { code: code.unwrap_or(1000), reason }
    }
}

impl std::fmt::Display for WebsocketCloseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: websocket.close")?;
        writeln!(f, "code: {}", self.code)?;
        writeln!(f, "reason: {}", self.reason)?;
        Ok(())
    }
}