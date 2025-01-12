use std::sync::Arc;

use asgispec::prelude::*;
use derive_more::derive::Constructor;
use log::{error, warn};
use tokio::sync::oneshot::{error::TryRecvError, Receiver as OneshotReceiver};

use crate::error::{Result, Error};

#[derive(Constructor)]
pub(crate) struct ApplicationWrapper<A: ASGIApplication + 'static> {
    inner: A,
    send: SendFn,
    receive: ReceiveFn,
    send_queue: async_channel::Sender<ASGIReceiveEvent>,
    receive_queue: async_channel::Receiver<ASGISendEvent>,
}

impl<A: ASGIApplication> ApplicationWrapper<A> {
    pub fn call(self, scope: Scope<A::State>) -> RunningApplication {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let queue = self.receive_queue.clone();

        tokio::task::spawn(async move {
            let out = if let Err(e) = self.inner.call(scope, self.receive, self.send).await {
                error!("Application error: {e}");
                Err(Error::custom(e.to_string()))
            } else {
                Ok(())
            };
            let _ = tx.send(out);
            queue.close();
        });

        RunningApplication::new(rx, self.send_queue, self.receive_queue)
    }
}

impl<A: ASGIApplication> From<&A> for ApplicationWrapper<A> {
    fn from(application: &A) -> Self {
        let (app_tx, server_rx) = async_channel::unbounded();
        let (server_tx, app_rx) = async_channel::unbounded();
    
        let receive_closure = move || -> ReceiveFuture {
            let rxc = app_rx.clone();
            Box::new(Box::pin(async move {
                match rxc.recv().await {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!("{e}");
                        ASGIReceiveEvent::new_http_disconnect()
                    }
                }
            }))
        };
    
        let send_closure = move |message: ASGISendEvent| -> SendFuture {
            let txc = app_tx.clone();
            Box::new(Box::pin(async move {
                if txc.send(message).await.is_err() {
                    return Err(Error::disconnected_client().into());
                }
                Ok(())
            }))
        };
    
        Self::new(
            application.clone(),
            Arc::new(send_closure),
            Arc::new(receive_closure),
            server_tx,
            server_rx,
        )
    }
}

#[derive(Constructor)]
pub(crate) struct RunningApplication {
    result_handle: OneshotReceiver<Result<()>>,
    send_queue: async_channel::Sender<ASGIReceiveEvent>,
    receive_queue: async_channel::Receiver<ASGISendEvent>,
}

impl RunningApplication {
    pub async fn close(&mut self) {
        self.receive_queue.close();
    }

    pub async fn send_to(&mut self, message: ASGIReceiveEvent) -> Result<()> {
        match self.result_handle.try_recv() {
            Ok(_) => return Err(Error::custom("Attempted to send message to application that has finshed")),
            Err(TryRecvError::Closed) => return Err(Error::custom("Attempted to send message to application that stopped")),
            Err(TryRecvError::Empty) => {}
        };

        if let Err(e) = self.send_queue.send(message).await {
            warn!("Failed to send message to app. {e}");
            return Err(Error::custom(e.to_string()));
        };
        Ok(())
    }

    pub async fn receive_from(&mut self) -> Option<ASGISendEvent> {
        match (self.result_handle.try_recv(), self.receive_queue.is_empty()) {
            (Ok(_), true) | (Err(TryRecvError::Closed), true) => return None,
            _ => {}
        };

        self.receive_queue.recv().await.ok()
    }
}