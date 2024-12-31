use std::io;
use std::sync::Arc;

use thiserror::Error;

use crate::{ASGIReceiveEvent, ASGISendEvent};

pub type Result<T> = std::result::Result<T, Error>;

// Errors the ASGI server could raise
#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("{0}")]
    DisconnectedClient(String),

    #[error(transparent)]
    Hyper(#[from] hyper::Error),

    #[error(transparent)]
    HTTP(#[from] http::Error),

    #[error("{src} shutdown unexpectedly. {reason}")]
    UnexpectedShutdown { src: String, reason: String },

    #[error(transparent)]
    IO(#[from] io::Error),

    #[error("Unexpected ASGI message received. {msg:?}")]
    UnexpectedASGIMessage {
        msg: Box<dyn std::fmt::Debug + Send + Sync>,
    },

    #[error(transparent)]
    ChannelReceiveError(#[from] async_channel::SendError<ASGIReceiveEvent>),

    #[error(transparent)]
    ChannelSendError(#[from] async_channel::SendError<ASGISendEvent>),

    #[error("Disconnect")]
    Disconnect,

    #[error(transparent)]
    SemaphoreAcquireError(#[from] tokio::sync::AcquireError),

    #[error(transparent)]
    WebsocketError(#[from] fastwebsockets::WebSocketError),
}

impl Error {
    pub fn custom(val: impl std::fmt::Display) -> Self {
        Self::Custom(val.to_string())
    }

    pub fn unexpected_asgi_message(msg: Box<dyn std::fmt::Debug + Send + Sync>) -> Self {
        Self::UnexpectedASGIMessage { msg }
    }

    pub fn disconnected_client() -> Self {
        Self::DisconnectedClient(String::from("Disconnected client"))
    }

    pub fn unexpected_shutdown(src: &str, reason: String) -> Self {
        Self::UnexpectedShutdown {
            src: src.to_string(),
            reason,
        }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::custom(value.to_string())
    }
}

impl Into<Arc<dyn std::error::Error + Send + Sync>> for Error {
    fn into(self) -> Arc<dyn std::error::Error + Send + Sync> {
        Arc::new(self)
    }
}