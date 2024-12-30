use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use tokio::sync::oneshot::Receiver as OneshotReceiver;
use derive_more::derive::Constructor;
use log::{error, warn};

use crate::asgispec::{ASGICallable, ASGIReceiveEvent, ASGISendEvent, ReceiveFn, Scope, SendFn, State};
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
        if let Err(e) = self.send_queue.try_send(message) {
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
pub struct Application<S: State + 'static, T: ASGICallable<S> + 'static> {
    asgi_callable: T,
    send: SendFn,
    receive: ReceiveFn,
    send_queue: async_channel::Sender<ASGIReceiveEvent>,
    receive_queue: async_channel::Receiver<ASGISendEvent>,
    phantom_data: PhantomData<S>,
}

impl<S: State, T: ASGICallable<S>> Application<S, T> {
    pub fn call(self, scope: Scope<S>) -> (RunningApplication, OneshotReceiver<Result<()>>) {
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
pub struct ApplicationFactory<S: State, T: ASGICallable<S>> {
    asgi_callable: T,
    phantom_data: PhantomData<S>,
}

impl<S: State, T: ASGICallable<S>> ApplicationFactory<S, T> {
    pub fn new(asgi_callable: T) -> Self {
        Self {
            asgi_callable,
            phantom_data: PhantomData,
        }
    }

    pub fn build(&self) -> Application<S, T> {
        let (app_tx, server_rx) = async_channel::bounded(64);
        let (server_tx, app_rx) = async_channel::bounded(64);

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

        let send_closure = move |message: ASGISendEvent| -> Box<dyn Future<Output = Result<()>> + Sync + Send + Unpin> {
            let txc = app_tx.clone();
            Box::new(Box::pin(async move {
                if let Err(_) = txc.send(message).await {
                    return Err(Error::disconnected_client());
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
