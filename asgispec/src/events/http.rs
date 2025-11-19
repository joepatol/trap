#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HTTPRequestEvent {
    pub body: Vec<u8>,
    pub more_body: bool,
}

impl HTTPRequestEvent {
    pub fn new(body: Vec<u8>, more_body: bool) -> Self {
        Self { body, more_body }
    }
}

impl std::fmt::Display for HTTPRequestEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: http.request")?;
        writeln!(f, "body:")?;
        writeln!(f, "   {}", String::from_utf8_lossy(&self.body))?;
        writeln!(f, "more_body: {}", self.more_body)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HTTPResponseStartEvent {
    pub status: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

impl HTTPResponseStartEvent {
    pub fn new(status: u16, headers: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Self { status, headers }
    }
}

impl std::fmt::Display for HTTPResponseStartEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: http.response.start")?;
        writeln!(f, "status: {}", self.status)?;
        writeln!(f, "headers:")?;
        for (name, value) in &self.headers {
            writeln!(
                f,
                "  {}: {}",
                String::from_utf8_lossy(name),
                String::from_utf8_lossy(value)
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HTTPResponseBodyEvent {
    pub body: Vec<u8>,
    pub more_body: bool,
}

impl HTTPResponseBodyEvent {
    pub fn new(body: Vec<u8>, more_body: bool) -> Self {
        Self { body, more_body }
    }
}

impl std::fmt::Display for HTTPResponseBodyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: http.response.body")?;
        writeln!(f, "body:")?;
        writeln!(f, "   {}", String::from_utf8_lossy(&self.body))?;
        writeln!(f, "more_body: {}", self.more_body)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HTTPDisconnectEvent;

impl HTTPDisconnectEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for HTTPDisconnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: http.disconnect")?;
        Ok(())
    }
}
