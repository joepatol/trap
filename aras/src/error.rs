use std::io;

use thiserror::Error;
use asgispec::prelude::*;

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
    WebsocketError(#[from] fastwebsockets::WebSocketError),
}

pub enum UnexpectedShutdownSrc {
    Application,
    Server,
}

impl From<UnexpectedShutdownSrc> for String {
    fn from(value: UnexpectedShutdownSrc) -> Self {
        match value {
            UnexpectedShutdownSrc::Application => String::from("Application"),
            UnexpectedShutdownSrc::Server => String::from("Server"),
        }
    }
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

    pub fn unexpected_shutdown(src: UnexpectedShutdownSrc, reason: String) -> Self {
        Self::UnexpectedShutdown {
            src: src.into(),
            reason,
        }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::custom(value.to_string())
    }
}