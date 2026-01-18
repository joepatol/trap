use serde::{Deserialize, Serialize};

use crate::spec::{ASGIScope, State};

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

impl<S: State> std::fmt::Display for LifespanScope<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "type: {}", "lifespan")?;
        writeln!(f, "asgi: {}", self.asgi)?;
        if self.state.is_some() {
            writeln!(f, "state: {}", self.state.clone().unwrap())?;
        } else {
            writeln!(f, "state: None")?;
        }
        Ok(())
    }
}