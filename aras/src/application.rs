use std::sync::Arc;

use asgispec::prelude::*;
use async_channel::{self as channel, Receiver, Sender};
use derive_more::derive::Constructor;
use log::{error, warn};
use tokio::sync::oneshot::{self, error::TryRecvError, Receiver as OneshotReceiver};

use crate::error::{Error, Result};

/// This struct wraps an `ASGIApplication` and exposes an API to interact with the application.
/// Communication is done through 2 mpmc channels, for server-application and application-server communication.
#[derive(Constructor)]
pub(crate) struct ApplicationWrapper<A: ASGIApplication + 'static> {
    inner: A,
    send: SendFn,
    receive: ReceiveFn,
    send_queue: Sender<ASGIReceiveEvent>,
    receive_queue: Receiver<ASGISendEvent>,
}

impl<A: ASGIApplication> ApplicationWrapper<A> {
    /// Call the inner applications `call` method providing send, receive and scope.
    pub fn call(self, scope: Scope<A::State>) -> CalledApplication {
        // Channel for the application to communicate it's output
        let (tx, rx) = oneshot::channel();
        let queue = self.receive_queue.clone();

        tokio::task::spawn(async move {
            let out = if let Err(e) = self.inner.call(scope, self.receive, self.send).await {
                error!("Application error: {e}");
                Err(Error::application_error(Box::new(format!("{e:?}"))))
            } else {
                Ok(())
            };
            // If the app has returned, send the output and close the receiver queue.
            // This prevents any new messages to be send.
            let _ = tx.send(out);
            queue.close();
        });

        CalledApplication::new(rx, self.send_queue, self.receive_queue)
    }
}

impl<A: ASGIApplication> From<A> for ApplicationWrapper<A> {
    fn from(value: A) -> Self {
        Self::from(&value)
    }
}

impl<A: ASGIApplication> From<&A> for ApplicationWrapper<A> {
    fn from(application: &A) -> Self {
        let (app_tx, server_rx) = channel::bounded(32);
        let (server_tx, app_rx) = channel::bounded(32);

        let receive_closure = move || -> ReceiveFuture {
            let rxc = app_rx.clone();
            Box::new(Box::pin(async move {
                match rxc.recv().await {
                    Ok(msg) => msg,
                    Err(_) => ASGIReceiveEvent::new_http_disconnect(),
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

/// An ASGIApplication that has been called
/// This type is mainly for convenience. It circumvents the need for a generic `A`
#[derive(Constructor)]
pub(crate) struct CalledApplication {
    pub(crate) result_handle: OneshotReceiver<Result<()>>,
    send_queue: Sender<ASGIReceiveEvent>,
    receive_queue: Receiver<ASGISendEvent>,
}

impl CalledApplication {
    /// Close communication with the application, by closing the receive queue.
    /// This prevents any new message to be send to the application.
    pub fn close(&mut self) {
        self.receive_queue.close();
    }

    /// Send a message to the application. This method will first check if the application is still
    /// running and return an error if not.
    pub async fn send_to(&mut self, message: ASGIReceiveEvent) -> Result<()> {
        match self.result_handle.try_recv() {
            // Application stopped between the previous communication and this one
            Ok(Ok(_)) => {
                return Err(Error::application_not_running())
            }
            // Application returned an error between the previous communication and this one
            Ok(Err(e)) => {
                return Err(e)
            }
            // Application stopped before the previous communication
            Err(TryRecvError::Closed) => {
                return Err(Error::application_not_running())
            }
            // Application is still running
            Err(TryRecvError::Empty) => {}
        };

        if let Err(e) = self.send_queue.send(message).await {
            warn!("Failed to send message to app. {e}");
            return Err(Error::custom(e.to_string()));
        };
        Ok(())
    }

    /// Receive a message from the application. If a message is on the queue it will immediately be received and
    /// returned. If not, this method will check if any more message can be received by checking if the application has returned.
    /// If the application has returned, an error is returned. If not, this method will wait for a new message to arrive.  
    pub async fn receive_from(&mut self) -> Result<ASGISendEvent> {
        // If there is a message, receive it
        if !self.receive_queue.is_empty() {
            return Ok(self.receive_queue.recv().await?);
        }

        // Check if the application is still running and if so, wait for a message
        match self.result_handle.try_recv() {
            // There is no return message, but the app is still running, so wait for one
            Err(TryRecvError::Empty) => Ok(self.receive_queue.recv().await?),
            // The app returned
            Ok(Ok(_)) => Err(Error::application_not_running()),
            // The app returned with an error
            Ok(Err(e)) => Err(e),
            // There is no return message because the channel is closed or it was already received
            Err(TryRecvError::Closed) => Err(Error::application_not_running()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use asgispec::prelude::*;
    use asgispec::scope::LifespanScope;

    use super::{ApplicationWrapper, CalledApplication};

    use crate::applications::*;
    use crate::error::Result;

    fn build_lifespan_scope() -> Scope<MockState> {
        Scope::Lifespan(LifespanScope::new(ASGIScope::default(), Some(MockState {})))
    }

    async fn wait_for_application_output(app: &mut CalledApplication) -> Result<()> {
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
        called_app.close();
        called_app
            .send_to(ASGIReceiveEvent::new_http_disconnect())
            .await
            .unwrap();

        let result = wait_for_application_output(&mut called_app).await;
        assert!(result.is_err_and(|e| e.to_string() == "Disconnected client"));
    }

    #[tokio::test]
    async fn test_app_returns_error() {
        let wrapper = ApplicationWrapper::from(ErrorOnCallApp {});
        let mut called_app = wrapper.call(build_lifespan_scope());

        let result = wait_for_application_output(&mut called_app).await;
        assert!(result.is_err_and(|e| e.to_string() == "Immediate error"));
        assert!(called_app
            .send_to(ASGIReceiveEvent::new_http_disconnect())
            .await
            .is_err_and(|e| { e.to_string() == "Attempted to send message to application that stopped" }));
    }

    #[tokio::test]
    async fn test_app_returns_ok() {
        let wrapper = ApplicationWrapper::from(ImmediateReturnApp {});
        let mut called_app = wrapper.call(build_lifespan_scope());

        let result = wait_for_application_output(&mut called_app).await;
        assert!(result.is_ok());
        assert!(called_app
            .send_to(ASGIReceiveEvent::new_http_disconnect())
            .await
            .is_err_and(|e| { e.to_string() == "Attempted to send message to application that stopped" }));
    }
}
