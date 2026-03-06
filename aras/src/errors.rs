use std::fmt::{Debug, Display};
use std::io;
use std::sync::Arc;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

pub trait DebugDisplay: Debug + Display {}
impl<T: Debug + Display> DebugDisplay for T {}

// Errors the ASGI server could raise
#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error(transparent)]
    Hyper(#[from] Arc<hyper::Error>),

    #[error(transparent)]
    HTTP(#[from] Arc<http::Error>),

    #[error(transparent)]
    IO(#[from] Arc<io::Error>),

    #[error("Unexpected ASGI message received. {msg:?}")]
    UnexpectedASGIMessage {
        msg: Arc<dyn DebugDisplay + Send + Sync>,
    },

    #[error("Disconnect")]
    Disconnect,

    #[error(transparent)]
    WebsocketError(#[from] Arc<fastwebsockets::WebSocketError>),

    #[error("Application error: {msg}")]
    ApplicationError {
        msg: Arc<dyn DebugDisplay + Send + Sync>,
    },

    #[error("Application is not running")]
    ApplicationNotRunning,

    #[error("ASGI await timeout elapsed")]
    Timeout(#[from] Arc<tokio::time::error::Elapsed>),
}

impl Error {
    pub fn custom(val: impl Display) -> Self {
        Self::Custom(val.to_string())
    }

    pub fn application_error(msg: Arc<dyn DebugDisplay + Send + Sync>) -> Self {
        Self::ApplicationError { msg }
    }

    pub fn application_not_running() -> Self {
        Self::ApplicationNotRunning
    }

    pub fn unexpected_asgi_message(msg: Arc<dyn DebugDisplay + Send + Sync>) -> Self {
        Self::UnexpectedASGIMessage { msg }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::custom(value.to_string())
    }
}

impl From<hyper::Error> for Error {
    fn from(value: hyper::Error) -> Self {
        Self::Hyper(Arc::new(value))
    }
}

impl From<http::Error> for Error {
    fn from(value: http::Error) -> Self {
        Self::HTTP(Arc::new(value))
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IO(Arc::new(value))
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
