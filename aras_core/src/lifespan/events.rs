#[derive(Debug, Clone)]
pub struct LifespanStartup {
    pub type_: String,
}

impl LifespanStartup {
    pub fn new() -> Self {
        Self { type_: "lifespan.startup".into() }
    }
}

#[derive(Debug, Clone)]
pub struct LifespanStartupComplete {
    pub type_: String,
}

impl LifespanStartupComplete {
    pub fn new() -> Self {
        Self { type_: "lifespan.startup.complete".into() }
    }
}

#[derive(Debug, Clone)]
pub struct LifespanStartupFailed {
    pub type_: String,
    pub message: String,
}

impl LifespanStartupFailed {
    pub fn new(message: String) -> Self {
        Self { type_: "lifespan.startup.failed".into(), message }
    }
}

#[derive(Debug, Clone)]
pub struct LifespanShutdown {
    pub type_: String,
}

impl LifespanShutdown {
    pub fn new() -> Self {
        Self { type_: "lifespan.shutdown".into() }
    }
}

#[derive(Debug, Clone)]
pub struct LifespanShutdownComplete {
    pub type_: String,
}

impl LifespanShutdownComplete {
    pub fn new() -> Self {
        Self { type_: "lifespan.shutdown.complete".into() }
    }
}

#[derive(Debug, Clone)]
pub struct LifespanShutdownFailed {
    pub type_: String,
    pub message: String,
}

impl LifespanShutdownFailed {
    pub fn new(message: String) -> Self {
        Self { type_: "lifespan.shutdown.failed".into(), message }
    }
}