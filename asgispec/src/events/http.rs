use bytes::Bytes;

use crate::spec::DisplaySerde;
use serde::{Deserialize, Serialize};

fn _default_false() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HTTPRequestEvent {
    pub body: Bytes,
    #[serde(default = "_default_false")]
    pub more_body: bool,
}

impl HTTPRequestEvent {
    pub fn new(body: Bytes, more_body: bool) -> Self {
        Self { body, more_body }
    }
}

impl std::fmt::Display for HTTPRequestEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HTTPResponseStartEvent {
    pub status: u16,
    pub headers: Vec<(Bytes, Bytes)>,
}

impl HTTPResponseStartEvent {
    pub fn new(status: u16, headers: Vec<(Bytes, Bytes)>) -> Self {
        Self { status, headers }
    }
}

impl std::fmt::Display for HTTPResponseStartEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HTTPResponseBodyEvent {
    pub body: Bytes,
    #[serde(default = "_default_false")]
    pub more_body: bool,
}

impl HTTPResponseBodyEvent {
    pub fn new(body: Bytes, more_body: bool) -> Self {
        Self { body, more_body }
    }
}

impl std::fmt::Display for HTTPResponseBodyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HTTPDisconnectEvent;

impl HTTPDisconnectEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for HTTPDisconnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}
