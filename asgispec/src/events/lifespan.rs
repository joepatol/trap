use crate::spec::DisplaySerde;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifespanStartupEvent;

impl LifespanStartupEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for LifespanStartupEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifespanStartupCompleteEvent;

impl LifespanStartupCompleteEvent {
    pub fn new() -> Self {
        Self 
    }
}

impl std::fmt::Display for LifespanStartupCompleteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifespanShutdownEvent;

impl LifespanShutdownEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for LifespanShutdownEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifespanShutdownCompleteEvent;

impl LifespanShutdownCompleteEvent {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for LifespanShutdownCompleteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        DisplaySerde::from(self).fmt(f)
    }
}