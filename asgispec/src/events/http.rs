#[derive(Debug, Clone)]
pub struct HTTPRequestEvent {
    pub type_: String,
    pub body: Vec<u8>,
    pub more_body: bool,
}

impl HTTPRequestEvent {
    pub fn new(body: Vec<u8>, more_body: bool) -> Self {
        Self {
            type_: "http.request".into(),
            body,
            more_body,
        }
    }
}

impl std::fmt::Display for HTTPRequestEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "body:")?;
        writeln!(f, "   {}", String::from_utf8_lossy(&self.body))?;
        writeln!(f, "more_body: {}", self.more_body)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HTTPResponseStartEvent {
    pub type_: String,
    pub status: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

impl HTTPResponseStartEvent {
    pub fn new(status: u16, headers: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Self {
            type_: "http.response.start".into(),
            status,
            headers,
        }
    }
}

impl std::fmt::Display for HTTPResponseStartEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "status: {}", self.status)?;
        writeln!(f, "headers:")?;
        for (name, value) in &self.headers {
            writeln!(f, "  {}: {}", String::from_utf8_lossy(name), String::from_utf8_lossy(value))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HTTPResonseBodyEvent {
    pub type_: String,
    pub body: Vec<u8>,
    pub more_body: bool,
}

impl HTTPResonseBodyEvent {
    pub fn new(body: Vec<u8>, more_body: bool) -> Self {
        Self {
            type_: "http.response.body".into(),
            body,
            more_body,
        }
    }
}

impl std::fmt::Display for HTTPResonseBodyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "body:")?;
        writeln!(f, "   {}", String::from_utf8_lossy(&self.body))?;
        writeln!(f, "more_body: {}", self.more_body)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HTTPDisconnectEvent {
    pub type_: String,
}

impl HTTPDisconnectEvent {
    pub fn new() -> Self {
        Self { type_: "http.disconnect".into() }
    }
}

impl std::fmt::Display for HTTPDisconnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        Ok(())
    }
}