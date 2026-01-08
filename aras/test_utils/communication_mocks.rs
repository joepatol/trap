use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::Mutex;

use asgispec::prelude::*;
use crate::errors::{Result, Error};
use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};

pub(crate) struct SendToAppFail;

impl SendToAppFail {
    pub fn new() -> Self {
        Self {}
    }
}

impl SendToASGIApp for SendToAppFail {
    async fn send(&mut self, _message: ASGIReceiveEvent) -> Result<()> {
        Err(Error::custom("SendToAppFail always fails"))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SendToAppCollector {
    messages: Arc<Mutex<Vec<ASGIReceiveEvent>>>,
}

impl SendToAppCollector {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_messages(&self) -> Vec<ASGIReceiveEvent> {
        let lock = self.messages.lock().await;
        lock.clone()
    }
}

impl SendToASGIApp for SendToAppCollector {
    async fn send(&mut self, message: ASGIReceiveEvent) -> Result<()> {
        let mut lock = self.messages.lock().await;
        lock.push(message);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DeterministicReceiveFromApp {
    messages: Vec<Result<ASGISendEvent>>,
    index: Arc<Mutex<usize>>,
}

impl Default for DeterministicReceiveFromApp {
    fn default() -> Self {
        let msg1 = ASGISendEvent::new_http_response_start(200, Vec::new());
        let msg2 = ASGISendEvent::new_http_response_body(Bytes::from(""), false);
        Self::new(vec![Ok(msg1), Ok(msg2)])
    }
}

impl DeterministicReceiveFromApp {
    pub fn new(messages: Vec<Result<ASGISendEvent>>) -> Self {
        Self {
            messages,
            index: Arc::new(Mutex::new(0)),
        }
    }
}

impl ReceiveFromASGIApp for DeterministicReceiveFromApp {
    async fn receive(&mut self) -> Result<ASGISendEvent> {
        let mut index_lock = self.index.lock().await;

        if *index_lock >= self.messages.len() {
            // Simulate waiting for more messages that never arrive
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            return Err(Error::custom("No more messages to receive"));
        }

        let message = self.messages[*index_lock].clone();
        *index_lock += 1;
        message
    }
}
