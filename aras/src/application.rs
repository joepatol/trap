use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use tokio::sync::oneshot::Receiver as OneshotReceiver;
use derive_more::derive::Constructor;
use log::{error, warn};
use asgispec::prelude::*;

use crate::error::Result;
use crate::Error;

#[derive(Constructor, Clone)]
pub struct RunningApplication {
    send_queue: async_channel::Sender<ASGIReceiveEvent>,
    receive_queue: async_channel::Receiver<ASGISendEvent>,
}

impl RunningApplication {
    pub async fn close(&mut self) {
        self.receive_queue.close();
    }

    pub async fn send_to(&self, message: ASGIReceiveEvent) -> Result<()> {
        let full = self.send_queue.is_full();
        println!("Channel full: {full}");
        let rec_c = self.send_queue.receiver_count();
        println!("Channel receiver count: {rec_c}");
        println!("Channel sender count: {}", self.send_queue.sender_count());
        println!("Channel closed: {}", self.send_queue.is_closed());
        if let Err(e) = self.send_queue.send(message).await {
            warn!("Failed to send message to app. {e}");
            return Err(Error::custom(e.to_string()))
        };
        println!("send");
        Ok(())
    }

    pub async fn receive_from(&mut self) -> Option<ASGISendEvent> {
        self.receive_queue.recv().await.ok()
    }
}

#[derive(Constructor, Clone)]
pub struct Application<S: State + 'static, T: ASGIApplication<S> + 'static> {
    asgi_callable: T,
    send: SendFn,
    receive: ReceiveFn,
    send_queue: async_channel::Sender<ASGIReceiveEvent>,
    receive_queue: async_channel::Receiver<ASGISendEvent>,
    phantom_data: PhantomData<S>,
}

impl<S: State, T: ASGIApplication<S>> Application<S, T> {
    pub fn call(self, scope: Scope<S>) -> (RunningApplication, OneshotReceiver<ASGIResult<()>>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let queue = self.receive_queue.clone();

        tokio::task::spawn(async move {
            let out = if let Err(e) = self.asgi_callable.call(scope, self.receive, self.send).await {
                error!("Application error: {e}");
                Err(e)
            } else {
                Ok(())
            };
            queue.close();
            let _ = tx.send(out);
        });

        (RunningApplication::new(self.send_queue, self.receive_queue), rx)
    }
}

#[derive(Clone)]
pub struct ApplicationFactory<S: State, T: ASGIApplication<S>> {
    asgi_callable: T,
    phantom_data: PhantomData<S>,
}

impl<S: State, T: ASGIApplication<S>> ApplicationFactory<S, T> {
    pub fn new(asgi_callable: T) -> Self {
        Self {
            asgi_callable,
            phantom_data: PhantomData,
        }
    }

    pub fn build(&self) -> Application<S, T> {
        let (app_tx, server_rx) = async_channel::unbounded();
        let (server_tx, app_rx) = async_channel::unbounded();

        let receive_closure = move || -> Box<dyn Future<Output = ASGIReceiveEvent> + Sync + Send + Unpin> {
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

        let send_closure = move |message: ASGISendEvent| -> Box<dyn Future<Output = ASGIResult<()>> + Sync + Send + Unpin> {
            let txc = app_tx.clone();
            Box::new(Box::pin(async move {
                if let Err(_) = txc.send(message).await {
                    return Err(Error::disconnected_client().into());
                }
                Ok(())
            }))
        };

        Application::new(
            self.asgi_callable.clone(),
            Arc::new(send_closure),
            Arc::new(receive_closure),
            server_tx,
            server_rx,
            PhantomData,
        )
    }
}
