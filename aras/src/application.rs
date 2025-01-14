use std::sync::Arc;

use asgispec::prelude::*;
use derive_more::derive::Constructor;
use log::{error, warn};
use tokio::sync::oneshot::{self, error::TryRecvError, Receiver as OneshotReceiver};
use async_channel::{self as channel, Sender, Receiver};

use crate::error::{Error, Result};

#[derive(Constructor)]
pub(crate) struct ApplicationWrapper<A: ASGIApplication + 'static> {
    inner: A,
    send: SendFn,
    receive: ReceiveFn,
    send_queue: Sender<ASGIReceiveEvent>,
    receive_queue: Receiver<ASGISendEvent>,
}

impl<A: ASGIApplication> ApplicationWrapper<A> {
    pub fn call(self, scope: Scope<A::State>) -> RunningApplication {
        let (tx, rx) = oneshot::channel();
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

impl<A: ASGIApplication> From<A> for ApplicationWrapper<A> {
    fn from(value: A) -> Self {
        Self::from(&value)
    }
}

impl<A: ASGIApplication> From<&A> for ApplicationWrapper<A> {
    fn from(application: &A) -> Self {
        let (app_tx, server_rx) = channel::unbounded();
        let (server_tx, app_rx) = channel::unbounded();

        let receive_closure = move || -> ReceiveFuture {
            let rxc = app_rx.clone();
            Box::new(Box::pin(async move {
                match rxc.recv().await {
                    Ok(msg) => msg,
                    Err(_) => {
                        ASGIReceiveEvent::new_http_disconnect()
                    }
                }
            }))
        };

        let send_closure = move |message: ASGISendEvent| -> SendFuture {
            let txc = app_tx.clone();
            Box::new(Box::pin(async move {
                if txc.send(message).await.is_err() {
                    return Err(DisconnectedClient);
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
    pub(crate) result_handle: OneshotReceiver<Result<()>>,
    send_queue: Sender<ASGIReceiveEvent>,
    receive_queue: Receiver<ASGISendEvent>,
}

impl RunningApplication {
    pub async fn close(&mut self) {
        self.receive_queue.close();
    }

    pub async fn send_to(&mut self, message: ASGIReceiveEvent) -> Result<()> {
        match self.result_handle.try_recv() {
            Ok(_) => {
                return Err(Error::custom(
                    "Attempted to send message to application that has finshed",
                ))
            }
            Err(TryRecvError::Closed) => {
                return Err(Error::custom("Attempted to send message to application that stopped"))
            }
            Err(TryRecvError::Empty) => {}
        };

        println!("Sending msg to app");
        if let Err(e) = self.send_queue.try_send(message) {
            warn!("Failed to send message to app. {e}");
            return Err(Error::custom(e.to_string()));
        };
        println!("Message send");
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use asgispec::prelude::*;
    use asgispec::scope::LifespanScope;

    use super::{ApplicationWrapper, RunningApplication};

    use crate::error::Result;
    use crate::applications::*;

    fn build_lifespan_scope() -> Scope<MockState> {
        Scope::Lifespan(LifespanScope::new(ASGIScope::default(), Some(MockState {})))
    }

    async fn wait_for_application_output(app: &mut RunningApplication) -> Result<()> {
        loop {
            let result = app.result_handle.try_recv();
            match result {
                Ok(v) => return v,
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    #[tokio::test]
    async fn test_send_is_error_after_close() {
        let wrapper = ApplicationWrapper::from(AssertSendErrorApp {});
        let mut called_app = wrapper.call(build_lifespan_scope());
        called_app.close().await;
        called_app.send_to(ASGIReceiveEvent::new_http_disconnect()).await.unwrap();

        let result = wait_for_application_output(&mut called_app).await;
        assert!(result.is_err_and(|e| e.to_string() == "Disconnected client"));

    }

    #[tokio::test]
    async fn test_app_returns_error() {
        let wrapper = ApplicationWrapper::from(ErrorOnCallApp {});
        let mut called_app = wrapper.call(build_lifespan_scope());

        let result = wait_for_application_output(&mut called_app).await;
        assert!(result.is_err_and(|e| e.to_string() == "Immediate error"));
        assert!(called_app.send_to(ASGIReceiveEvent::new_http_disconnect()).await.is_err_and(|e| {
            e.to_string() == "Attempted to send message to application that stopped"
        }));
    }

    #[tokio::test]
    async fn test_app_returns_ok() {
        let wrapper = ApplicationWrapper::from(ImmediateReturnApp {});
        let mut called_app = wrapper.call(build_lifespan_scope());

        let result = wait_for_application_output(&mut called_app).await;
        assert!(result.is_ok());
        assert!(called_app.send_to(ASGIReceiveEvent::new_http_disconnect()).await.is_err_and(|e| {
            e.to_string() == "Attempted to send message to application that stopped"
        }));
    }
}