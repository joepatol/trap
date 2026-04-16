use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::Mutex;

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp, ApplicationResult, ApplicationError, CommunicationResult};
use asgispec::prelude::*;

pub(crate) struct SendToAppFail;

impl SendToAppFail {
    pub fn new() -> Self {
        Self {}
    }
}

impl SendToASGIApp for SendToAppFail {
    async fn send(&mut self, _message: ASGIReceiveEvent) -> CommunicationResult<()> {
        let e = ApplicationError::new("SendToAppFail always fails".to_string());
        Err(ApplicationResult::Failed(e))
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
    async fn send(&mut self, message: ASGIReceiveEvent) -> CommunicationResult<()> {
        let mut lock = self.messages.lock().await;
        lock.push(message);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DeterministicReceiveFromApp {
    messages: Vec<CommunicationResult<ASGISendEvent>>,
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
    pub fn new(messages: Vec<CommunicationResult<ASGISendEvent>>) -> Self {
        Self {
            messages,
            index: Arc::new(Mutex::new(0)),
        }
    }
}

impl ReceiveFromASGIApp for DeterministicReceiveFromApp {
    async fn receive(&mut self) -> CommunicationResult<ASGISendEvent> {
        let mut index_lock = self.index.lock().await;

        if *index_lock >= self.messages.len() {
            // Simulate waiting for more messages that never arrive
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let e = ApplicationError::new("No more messages to receive".to_string());
            return Err(ApplicationResult::Failed(e));
        }

        let message = self.messages[*index_lock].clone();
        *index_lock += 1;
        message
    }
}
