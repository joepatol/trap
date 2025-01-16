#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifespanStartupEvent;

impl LifespanStartupEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for LifespanStartupEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: lifespan.startup")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifespanStartupCompleteEvent;

impl LifespanStartupCompleteEvent {
    pub fn new() -> Self {
        Self 
    }
}

impl std::fmt::Display for LifespanStartupCompleteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: lifespan.startup.complete")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifespanStartupFailedEvent {
    pub message: String,
}

impl LifespanStartupFailedEvent {
    pub fn new(message: String) -> Self {
        Self {
            message,
        }
    }
}

impl std::fmt::Display for LifespanStartupFailedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: lifespan.startup.failed")?;
        writeln!(f, "message: {}", self.message)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifespanShutdownEvent;

impl LifespanShutdownEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for LifespanShutdownEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: lifespan.shutdown")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifespanShutdownCompleteEvent;

impl LifespanShutdownCompleteEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for LifespanShutdownCompleteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: lifespan.shutdown.complete")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifespanShutdownFailedEvent {
    pub message: String,
}

impl LifespanShutdownFailedEvent {
    pub fn new(message: String) -> Self {
        Self {
            message,
        }
    }
}

impl std::fmt::Display for LifespanShutdownFailedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: lifespan.shutdown.failed")?;
        writeln!(f, "message: {}", self.message)?;
        Ok(())
    }
}
