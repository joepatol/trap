use std::fmt::{Debug, Display};
use std::sync::Arc;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

pub trait DebugDisplay: Debug + Display {}
impl<T: Debug + Display> DebugDisplay for T {}

// Errors the ASGI server could raise
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// Ad-hoc error
    #[error("{0}")]
    Custom(String),

    /// Protocol errors
    #[error(transparent)]
    HTTP(#[from] Arc<http::Error>),

    #[error(transparent)]
    WebsocketError(#[from] Arc<fastwebsockets::WebSocketError>),

    /// Application errors
    #[error("{0}")]
    ApplicationError(Arc<dyn DebugDisplay + Send + Sync>),

    #[error("Application is not running")]
    ApplicationNotRunning,

    /// ASGI protocol error
    #[error("Unexpected ASGI message received. {0}")]
    UnexpectedASGIMessage(Arc<dyn DebugDisplay + Send + Sync>),

    #[error("{0}")]
    StartupFailed(Arc<dyn DebugDisplay + Send + Sync>),

    #[error("{0}")]
    ShutdownFailed(Arc<dyn DebugDisplay + Send + Sync>),

    /// Backpressure error
    #[error("ASGI await timeout elapsed")]
    Timeout(#[from] Arc<tokio::time::error::Elapsed>),
}

impl Error {
    pub fn custom(val: impl Display) -> Self {
        Self::Custom(val.to_string())
    }

    pub fn application_error(msg: &str) -> Self {
        Self::ApplicationError(Arc::new(msg.to_string()))
    }

    pub fn application_not_running() -> Self {
        Self::ApplicationNotRunning
    }

    pub fn unexpected_asgi_message(msg: &str) -> Self {
        Self::UnexpectedASGIMessage(Arc::new(msg.to_string()))
    }

    pub fn startup_failed(msg: &str) -> Self {
        Self::StartupFailed(Arc::new(msg.to_string()))
    }

    pub fn shutdown_failed(msg: &str) -> Self {
        Self::ShutdownFailed(Arc::new(msg.to_string()))
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::custom(value.to_string())
    }
}

impl From<http::Error> for Error {
    fn from(value: http::Error) -> Self {
        Self::HTTP(Arc::new(value))
    }
}

impl From<fastwebsockets::WebSocketError> for Error {
    fn from(value: fastwebsockets::WebSocketError) -> Self {
        Self::WebsocketError(Arc::new(value))
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(value: tokio::time::error::Elapsed) -> Self {
        Self::Timeout(Arc::new(value))
    }
}
