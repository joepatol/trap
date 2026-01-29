use serde::{Deserialize, Serialize};

use crate::spec::{ASGIScope, State, DisplaySerde};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifespanScope<S: State> {
    pub asgi: ASGIScope,
    pub state: Option<S>,
}

impl<S: State> LifespanScope<S> {
    pub fn new(asgi: ASGIScope, state: Option<S>) -> Self {
        Self { asgi, state }
    }
}

impl<S: State + Serialize> std::fmt::Display for LifespanScope<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplaySerde::from(self).fmt(f)
    }
}