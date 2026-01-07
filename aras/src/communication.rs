use std::sync::Arc;

use asgispec::prelude::*;
use async_channel::{self as channel, Receiver, Sender};
use derive_more::derive::Constructor;
use log::error;
use tokio::sync::oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender};
use tokio::sync::Mutex;

use crate::errors::{Error, Result};

const CHANNEL_SIZE: usize = 16;

#[derive(Constructor, Debug)]
pub struct ReceiveFromApp {
    state: ApplicationState,
    receiver: Receiver<ASGISendEvent>,
}

impl ReceiveFromApp {
    pub async fn next(&mut self) -> Result<ASGISendEvent> {
        if !self.receiver.is_empty() {
            return self.receiver.recv().await.map_err(Error::from);
        }

        if let Some(err) = self.state.ensure_ok().await {
            return Err(err);
        }
        self.receiver.recv().await.map_err(Error::from)
    }
}

#[derive(Constructor, Debug)]
pub struct SendToApp {
    state: ApplicationState,
    sender: Sender<ASGIReceiveEvent>,
}

impl SendToApp {
    pub async fn push(&mut self, message: ASGIReceiveEvent) -> Result<()> {
        if let Some(err) = self.state.ensure_ok().await {
            return Err(err);
        }
        self.sender.send(message).await.map_err(Error::from)
    }
}

#[derive(Clone, Debug)]
struct ApplicationState {
    error_value: Option<Error>,
    receiver: Arc<Mutex<OneshotReceiver<Result<()>>>>,
}

impl ApplicationState {
    pub fn new(receiver: OneshotReceiver<Result<()>>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(receiver)),
            error_value: None,
        }
    }

    async fn check(&mut self) {
        tokio::task::yield_now().await;
        let mut lock = self.receiver.lock().await;
        match lock.try_recv() {
            Ok(Ok(_)) => {
                self.error_value = Some(Error::application_not_running());
            }
            Ok(Err(e)) => {
                self.error_value = Some(e);
            }
            // TODO: what if dropped before sending a value
            _ => { /* Still running */ }
        }
    }

    pub async fn ensure_ok(&mut self) -> Option<Error> {
        if self.error_value.is_some() {
            return self.error_value.clone();
        };
        self.check().await;
        self.error_value.clone()
    }
}

pub struct CommunicatorFactory {
    send_to_server: Sender<ASGISendEvent>,
    receive_from_server: Receiver<ASGIReceiveEvent>,
    send_to_app: Sender<ASGIReceiveEvent>,
    receive_from_app: Receiver<ASGISendEvent>,
    result_producer: OneshotSender<Result<()>>,
    result_consumer: OneshotReceiver<Result<()>>,
}

impl CommunicatorFactory {
    pub fn new() -> Self {
        let (send_to_server, receive_from_app) = channel::bounded(CHANNEL_SIZE);
        let (send_to_app, receive_from_server) = channel::bounded(CHANNEL_SIZE);
        let (result_producer, result_consumer) = oneshot::channel();

        Self {
            send_to_server,
            receive_from_server,
            send_to_app,
            receive_from_app,
            result_producer,
            result_consumer,
        }
    }

    pub fn build<A: ASGIApplication + 'static>(self, application: A, scope: Scope<A::State>) -> (SendToApp, ReceiveFromApp) {
        let send_closure = move |message: ASGISendEvent| -> SendFuture {
            let txc = self.send_to_server.clone();
            Box::new(Box::pin(async move {
                if txc.send(message).await.is_err() {
                    return Err(DisconnectedClient);
                }
                Ok(())
            }))
        };

        let receive_closure = move || -> ReceiveFuture {
            let rxc = self.receive_from_server.clone();
            Box::new(Box::pin(async move {
                match rxc.recv().await {
                    Ok(msg) => msg,
                    Err(_) => ASGIReceiveEvent::new_http_disconnect(),
                }
            }))
        };

        let send = Arc::new(send_closure);
        let receive = Arc::new(receive_closure);

        tokio::task::spawn(async move {
            let out = if let Err(e) = application.call(scope, receive, send).await {
                error!("Application error: {e}");
                Err(Error::application_error(Arc::new(format!("{e:?}"))))
            } else {
                Ok(())
            };
            // Send the return value back to the communicator
            let _ = self.result_producer.send(out);
        });

        let state = ApplicationState::new(self.result_consumer);

        (SendToApp::new(state.clone(), self.send_to_app), ReceiveFromApp::new(state, self.receive_from_app))
    }
}
