use core::panic;
use std::future::Future;
use std::sync::Arc;

use asgispec::prelude::*;
use async_channel::{self as channel, Receiver, Sender};
use derive_more::derive::Constructor;
use log::error;
use tokio::sync::oneshot::{self, Receiver as OneshotReceiver};
use tokio::sync::{Mutex, RwLock};

use crate::{ArasError, ArasResult};

/// Handle to send ASGI messages to an application
pub(crate) trait SendToASGIApp: Send + Sync {
    fn send(
        &mut self,
        message: ASGIReceiveEvent,
    ) -> impl Future<Output = ArasResult<()>> + Send + Sync;
}

/// Handle to receive ASGI messages from an application
pub(crate) trait ReceiveFromASGIApp: Send + Sync {
    fn receive(&mut self) -> impl Future<Output = ArasResult<ASGISendEvent>> + Send + Sync;
}

/// Handle to track the state of the ASGI application.
/// Can wait for the application to complete and retrieve the result of the application execution.
/// This type is idempotent and can be cloned to share the same state across multiple senders and receivers.
/// The error value is cached after the first retrieval.
#[derive(Clone, Debug)]
struct ApplicationHandle {
    error_value: Arc<RwLock<Option<ArasError>>>,
    receiver: Arc<Mutex<Option<OneshotReceiver<ArasResult<()>>>>>,
}

impl ApplicationHandle {
    pub fn new(receiver: OneshotReceiver<ArasResult<()>>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            error_value: Arc::new(RwLock::new(None)),
        }
    }

    async fn read_value(&self) -> Option<ArasError> {
        let read_value = self.error_value.read().await;
        read_value.clone()
    }

    pub async fn wait_for_completion(&mut self) -> ArasError {
        // Return the error value if it is already set, we do this check first to avoid unnecessarily
        // acquiring exclusive locks
        if let Some(value) = self.read_value().await {
            return value;
        };

        // If there is no value yet, we lock the Mutex to check if the receiver is still available
        let maybe_recv = self.receiver.lock().await.take();

        // If it's not available, it could be due to another caller having already awaited it and set `self.error_value`.
        // If both are `None`, then the implementation is incorrect.
        if maybe_recv.is_none() {
            if let Some(value) = self.read_value().await {
                return value;
            } else {
                panic!("ApplicationState implementation incorrect: receiver already taken and error_value is None");
            }
        }

        // Wait for the receiver to complete, set the error value and return it
        let error_value = match maybe_recv.unwrap().await {
            Ok(Ok(_)) => ArasError::application_not_running(),
            Ok(Err(e)) => e,
            Err(_) => ArasError::application_not_running(),
        };

        let mut write_value = self.error_value.write().await;
        *write_value = Some(error_value.clone());

        error_value
    }
}

/// Receive handle to get ASGI messages from an application.
/// Uses an async channel to receive messages from the application.
#[derive(Constructor, Debug)]
pub(crate) struct ReceiveFromApp {
    handle: ApplicationHandle,
    receiver: Receiver<ASGISendEvent>,
}

impl ReceiveFromASGIApp for ReceiveFromApp {
    async fn receive(&mut self) -> ArasResult<ASGISendEvent> {
        if let Ok(message) = self.receiver.recv().await {
            Ok(message)
        } else {
            Err(self.handle.wait_for_completion().await)
        }
    }
}

/// Send handle to send ASGI messages to an application.
/// Uses an async channel to send messages to the application.
#[derive(Constructor, Debug)]
pub(crate) struct SendToApp {
    handle: ApplicationHandle,
    sender: Sender<ASGIReceiveEvent>,
}

impl SendToASGIApp for SendToApp {
    async fn send(&mut self, message: ASGIReceiveEvent) -> ArasResult<()> {
        if self.sender.send(message).await.is_ok() {
            Ok(())
        } else {
            Err(self.handle.wait_for_completion().await)
        }
    }
}

const CHANNEL_SIZE: usize = 16;

/// Factory to create communication channels between server and ASGI application.
/// Provides a method to build 2 handles for sending and receiving ASGI messages to and from the
/// application, given an ASGI scope.
#[derive(Constructor, Clone)]
pub(crate) struct CommunicationFactory<A: ASGIApplication> {
    application: A,
}

impl<A: ASGIApplication + 'static> CommunicationFactory<A> {
    /// Build communication channels for the given ASGI scope.
    pub fn build(&self, scope: Scope<A::State>) -> (impl SendToASGIApp, impl ReceiveFromASGIApp) {
        let (send_to_server, receive_from_app) = channel::bounded(CHANNEL_SIZE);
        let (send_to_app, receive_from_server) = channel::bounded(CHANNEL_SIZE);
        let (result_producer, result_consumer) = oneshot::channel();

        let send_closure = move |message: ASGISendEvent| -> SendFuture {
            let txc = send_to_server.clone();
            Box::new(Box::pin(async move {
                if txc.send(message).await.is_err() {
                    return Err(DisconnectedClient);
                }
                Ok(())
            }))
        };

        let receive_closure = move || -> ReceiveFuture {
            let rxc = receive_from_server.clone();
            Box::new(Box::pin(async move {
                match rxc.recv().await {
                    Ok(msg) => msg,
                    Err(_) => ASGIReceiveEvent::new_http_disconnect(),
                }
            }))
        };

        let send = Arc::new(send_closure);
        let receive = Arc::new(receive_closure);
        let app = self.application.clone();

        tokio::task::spawn(async move {
            let out = if let Err(e) = app.call(scope, receive, send).await {
                error!("Application error: {e}");
                Err(ArasError::application_error(Arc::new(format!("{e:?}"))))
            } else {
                Ok(())
            };
            // Send the return value back to the communicator
            let _ = result_producer.send(out);
        });

        let handle = ApplicationHandle::new(result_consumer);

        (
            SendToApp::new(handle.clone(), send_to_app),
            ReceiveFromApp::new(handle, receive_from_app),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::CommunicationFactory;
    use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
    use crate::scope::ScopeFactory;
    use asgispec::prelude::*;

    use crate::mocks::application::*;

    #[tokio::test]
    async fn test_send_lifespan_startup_event() {
        let app = LifespanProtocolApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        let startup_event = ASGIReceiveEvent::new_lifespan_startup();
        send_to_app.send(startup_event).await.unwrap();

        let received_event = receive_from_app.receive().await.unwrap();
        assert!(received_event == ASGISendEvent::new_startup_complete());
    }

    #[tokio::test]
    async fn test_send_lifespan_shutdown_event() {
        let app = LifespanProtocolApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        let shutdown_event = ASGIReceiveEvent::new_lifespan_shutdown();
        send_to_app.send(shutdown_event).await.unwrap();

        let received_event = receive_from_app.receive().await.unwrap();
        assert!(received_event == ASGISendEvent::new_shutdown_complete());
    }

    #[tokio::test]
    async fn test_app_returns_without_sending_message_send_to() {
        let app = ImmediateReturnApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, _) = comm_factory.build(scope);

        let startup_event = ASGIReceiveEvent::new_lifespan_startup();
        let result = send_to_app.send(startup_event).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_app_returns_without_sending_message_receive_from() {
        let app = ImmediateReturnApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (_, mut receive_from_app) = comm_factory.build(scope);

        let result = receive_from_app.receive().await;

        assert!(result.is_err_and(|e| { e.to_string() == "Application is not running" }));
    }

    #[tokio::test]
    async fn test_app_raises_an_error_send_to() {
        let app = ImmediateErrorApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, _) = comm_factory.build(scope);

        let startup_event = ASGIReceiveEvent::new_lifespan_startup();
        let result = send_to_app.send(startup_event).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_app_raises_an_error_receive_from() {
        let app = ImmediateErrorApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (_, mut receive_from_app) = comm_factory.build(scope);

        let result = receive_from_app.receive().await;

        assert!(result.is_err_and(|e| {
            e.to_string() == "Application error: TestError(\"Immediate error\")"
        }));
    }

    #[tokio::test]
    async fn test_app_raises_an_error_receive_then_send() {
        let app = ImmediateErrorApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        let recv_result = receive_from_app.receive().await;
        let send_result = send_to_app
            .send(ASGIReceiveEvent::new_lifespan_shutdown())
            .await;

        assert!(recv_result.is_err());
        assert!(send_result.is_err());
    }

    #[tokio::test]
    async fn test_app_raises_an_error_send_then_receive() {
        let app = ImmediateErrorApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        let send_result = send_to_app
            .send(ASGIReceiveEvent::new_lifespan_shutdown())
            .await;
        let recv_result = receive_from_app.receive().await;

        assert!(recv_result.is_err());
        assert!(send_result.is_ok());
    }

    #[tokio::test]
    async fn test_app_raises_an_error_send_then_receive_with_yield() {
        let app = ImmediateErrorApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        // Calling send is racy, as the app task may not have completed yet meaning the receiver could still be open
        // To make sure the app is able to send it's result before send is called we yield to the scheduler
        tokio::task::yield_now().await;

        let send_result = send_to_app
            .send(ASGIReceiveEvent::new_lifespan_shutdown())
            .await;
        let recv_result = receive_from_app.receive().await;

        assert!(recv_result.is_err());
        assert!(send_result.is_err());
    }

    #[tokio::test]
    async fn test_error_value_is_correct() {
        let app = ImmediateReturnApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        // We should keep on getting the same error value after the app has returned
        // Calling send immediately is racy, as the app task may not have completed yet meaning the receiver could still be open
        let result = receive_from_app.receive().await;
        assert!(result.is_err_and(|e| { e.to_string() == "Application is not running" }));
        let result = receive_from_app.receive().await;
        assert!(result.is_err_and(|e| { e.to_string() == "Application is not running" }));
        let result = send_to_app
            .send(ASGIReceiveEvent::new_lifespan_startup())
            .await;
        assert!(result.is_err_and(|e| { e.to_string() == "Application is not running" }));
        let result = receive_from_app.receive().await;
        assert!(result.is_err_and(|e| { e.to_string() == "Application is not running" }));
        let result = send_to_app
            .send(ASGIReceiveEvent::new_http_disconnect())
            .await;
        assert!(result.is_err_and(|e| { e.to_string() == "Application is not running" }));
    }

    #[tokio::test]
    async fn test_error_in_application() {
        let app = ErrorApp::new();
        let state = MockState::new();

        let comm_factory = CommunicationFactory::new(app);
        let scope_factory = ScopeFactory::new(state);
        let scope = scope_factory.build_lifespan();

        let (mut send_to_app, mut receive_from_app) = comm_factory.build(scope);

        let send1 = send_to_app
            .send(ASGIReceiveEvent::new_lifespan_startup())
            .await;
        let recv1 = receive_from_app.receive().await;

        let send2 = send_to_app
            .send(ASGIReceiveEvent::new_lifespan_shutdown())
            .await;
        let recv2 = receive_from_app.receive().await;

        assert!(send1.is_ok());
        assert!(recv1.is_ok_and(|msg| { msg == ASGISendEvent::new_startup_complete() }));
        assert!(send2.is_err_and(|e| { e.to_string() == "Application error: TestError(\"Oops\")" }));
        assert!(recv2.is_err_and(|e| { e.to_string() == "Application error: TestError(\"Oops\")" }));
    }
}
