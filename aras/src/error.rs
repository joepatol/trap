use std::io;

use thiserror::Error;
use asgispec::prelude::*;
use async_channel::{SendError, RecvError};

pub type Result<T> = std::result::Result<T, Error>;

// Errors the ASGI server could raise
#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

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
    ChannelSendError(#[from] SendError<ASGISendEvent>),

    #[error(transparent)]
    ChannelReceiveError(#[from] RecvError),

    #[error("Disconnect")]
    Disconnect,

    #[error(transparent)]
    WebsocketError(#[from] fastwebsockets::WebSocketError),

    #[error("Application error: {msg:?}")]
    ApplicationError { 
        msg: Box<dyn std::fmt::Debug + Send + Sync> 
    },

    #[error("Application is not running")]
    ApplicationNotRunning,

    #[error(transparent)]
    DisconnectedClient(#[from] SendError<ASGIReceiveEvent>)
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

    pub fn application_error(msg: Box<dyn std::fmt::Debug + Send + Sync>) -> Self {
        Self::ApplicationError { msg }
    }

    pub fn application_not_running() -> Self {
        Self::ApplicationNotRunning
    }

    pub fn unexpected_asgi_message(msg: Box<dyn std::fmt::Debug + Send + Sync>) -> Self {
        Self::UnexpectedASGIMessage { msg }
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