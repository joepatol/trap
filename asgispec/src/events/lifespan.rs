#[derive(Debug, Clone)]
pub struct LifespanStartupEvent {
    pub type_: String,
}

impl LifespanStartupEvent {
    pub fn new() -> Self {
        Self {
            type_: "lifespan.startup".into(),
        }
    }
}

impl std::fmt::Display for LifespanStartupEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LifespanStartupCompleteEvent {
    pub type_: String,
}

impl LifespanStartupCompleteEvent {
    pub fn new() -> Self {
        Self {
            type_: "lifespan.startup.complete".into(),
        }
    }
}

impl std::fmt::Display for LifespanStartupCompleteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LifespanStartupFailedEvent {
    pub type_: String,
    pub message: String,
}

impl LifespanStartupFailedEvent {
    pub fn new(message: String) -> Self {
        Self {
            type_: "lifespan.startup.failed".into(),
            message,
        }
    }
}

impl std::fmt::Display for LifespanStartupFailedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "message: {}", self.message)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LifespanShutdownEvent {
    pub type_: String,
}

impl LifespanShutdownEvent {
    pub fn new() -> Self {
        Self {
            type_: "lifespan.shutdown".into(),
        }
    }
}

impl std::fmt::Display for LifespanShutdownEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LifespanShutdownCompleteEvent {
    pub type_: String,
}

impl LifespanShutdownCompleteEvent {
    pub fn new() -> Self {
        Self {
            type_: "lifespan.shutdown.complete".into(),
        }
    }
}

impl std::fmt::Display for LifespanShutdownCompleteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LifespanShutdownFailedEvent {
    pub type_: String,
    pub message: String,
}

impl LifespanShutdownFailedEvent {
    pub fn new(message: String) -> Self {
        Self {
            type_: "lifespan.shutdown.failed".into(),
            message,
        }
    }
}

impl std::fmt::Display for LifespanShutdownFailedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", self.type_)?;
        writeln!(f, "message: {}", self.message)?;
        Ok(())
    }
}
