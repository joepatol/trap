use std::io;
use std::fmt::{Debug, Display};

use thiserror::Error;
use asgispec::prelude::*;
use async_channel::{SendError, RecvError};

pub type Result<T> = std::result::Result<T, Error>;

pub trait DebugDisplay: Debug + Display {}
impl<T: Debug + Display> DebugDisplay for T {}


// Errors the ASGI server could raise
#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error(transparent)]
    Hyper(#[from] hyper::Error),

    #[error(transparent)]
    HTTP(#[from] http::Error),

    #[error(transparent)]
    IO(#[from] io::Error),

    #[error("Unexpected ASGI message received. {msg:?}")]
    UnexpectedASGIMessage {
        msg: Box<dyn DebugDisplay + Send + Sync>,
    },

    #[error(transparent)]
    ChannelSendError(#[from] SendError<ASGISendEvent>),

    #[error(transparent)]
    ChannelReceiveError(#[from] RecvError),

    #[error("Disconnect")]
    Disconnect,

    #[error(transparent)]
    WebsocketError(#[from] fastwebsockets::WebSocketError),

    #[error("Application error: {msg}")]
    ApplicationError { 
        msg: Box<dyn DebugDisplay + Send + Sync> 
    },

    #[error("Application is not running")]
    ApplicationNotRunning,

    #[error(transparent)]
    DisconnectedClient(#[from] SendError<ASGIReceiveEvent>)
}

impl Error {
    pub fn custom(val: impl Display) -> Self {
        Self::Custom(val.to_string())
    }

    pub fn application_error(msg: Box<dyn DebugDisplay + Send + Sync>) -> Self {
        Self::ApplicationError { msg }
    }

    pub fn application_not_running() -> Self {
        Self::ApplicationNotRunning
    }

    pub fn unexpected_asgi_message(msg: Box<dyn DebugDisplay + Send + Sync>) -> Self {
        Self::UnexpectedASGIMessage { msg }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::custom(value.to_string())
    }
}