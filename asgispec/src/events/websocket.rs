use bytes::Bytes;

use crate::spec::ASGIDisplay;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketConnectEvent;

impl WebsocketConnectEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for WebsocketConnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketAcceptEvent {
    pub subprotocol: Option<String>,
    pub headers: Vec<(Bytes, Bytes)>,
}

impl WebsocketAcceptEvent {
    pub fn new(
        subprotocol: Option<String>,
        headers: Vec<(Bytes, Bytes)>,
    ) -> Self {
        Self { subprotocol, headers }
    }
}

impl std::fmt::Display for WebsocketAcceptEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketReceiveEvent {
    pub bytes: Option<Bytes>,
    pub text: Option<String>,
}

impl WebsocketReceiveEvent {
    pub fn new(
        bytes: Option<Bytes>,
        text: Option<String>,
    ) -> Self {
        Self { bytes, text }
    } 
}

impl std::fmt::Display for WebsocketReceiveEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketSendEvent {
    pub bytes: Option<Bytes>,
    pub text: Option<String>,
}

impl WebsocketSendEvent {
    pub fn new(
        bytes: Option<Bytes>,
        text: Option<String>,
    ) -> Self {
        Self { bytes, text }
    } 
}

impl std::fmt::Display for WebsocketSendEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketDisconnectEvent {
    pub code: u16,
    pub reason: String,
}

impl WebsocketDisconnectEvent {
    pub fn new(code: u16, reason: String) -> Self {
        Self { code, reason }
    }
}

impl std::fmt::Display for WebsocketDisconnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsocketCloseEvent {
    pub code: u16,
    pub reason: String,
}

impl WebsocketCloseEvent {
    pub fn new(code: Option<u16>, reason: String) -> Self {
        Self { code: code.unwrap_or(1000), reason }
    }
}

impl std::fmt::Display for WebsocketCloseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ASGIDisplay::from(self).fmt(f)
    }
}