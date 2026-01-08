use std::future::Future;
use std::sync::Arc;

use asgispec::prelude::*;
use async_channel::{self as channel, Receiver, Sender};
use derive_more::derive::Constructor;
use log::error;
use tokio::sync::oneshot::{self, Receiver as OneshotReceiver};
use tokio::sync::Mutex;

use crate::errors::{Error, Result};

pub(crate) trait SendToASGIApp: Send + Sync {
    fn send(&mut self, message: ASGIReceiveEvent) -> impl Future<Output = Result<()>> + Send + Sync;
}

pub(crate) trait ReceiveFromASGIApp: Send + Sync {
    fn receive(&mut self) -> impl Future<Output = Result<ASGISendEvent>> + Send + Sync;
}

/// Keeps track of the internal state op the ASGI application. Either the application is still running or
/// it has returned. If it has returned, from the servers perspective this is always an error when
/// trying to communicate with the application.
/// So we keep an optional error value that is set when the application has returned.
/// Communication with the application is done via a oneshot receiver which should return
/// the result of the application task.
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

#[derive(Constructor, Debug)]
pub(crate) struct ReceiveFromApp {
    state: ApplicationState,
    receiver: Receiver<ASGISendEvent>,
}

impl ReceiveFromASGIApp for ReceiveFromApp {
    async fn receive(&mut self) -> Result<ASGISendEvent> {
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
pub(crate) struct SendToApp {
    state: ApplicationState,
    sender: Sender<ASGIReceiveEvent>,
}

impl SendToASGIApp for SendToApp {
    async fn send(&mut self, message: ASGIReceiveEvent) -> Result<()> {
        if let Some(err) = self.state.ensure_ok().await {
            return Err(err);
        }
        self.sender.send(message).await.map_err(Error::from)
    }
}

const CHANNEL_SIZE: usize = 16;

/// Factory to create communication channels between server and ASGI application.
/// Provides a method to build 2 handles for sending and receiving ASGI messages to and from the
/// application given an ASGI scope.
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
                Err(Error::application_error(Arc::new(format!("{e:?}"))))
            } else {
                Ok(())
            };
            // Send the return value back to the communicator
            let _ = result_producer.send(out);
        });

        let state = ApplicationState::new(result_consumer);

        (
            SendToApp::new(state.clone(), send_to_app),
            ReceiveFromApp::new(state, receive_from_app),
        )
    }
}

    // #[tokio::test]
    // async fn test_app_returns_when_called() {
    //     let handler = HTTPHandler::new(10);
    //     let request = Request::builder()
    //         .body("hello world".to_string())
    //         .expect("Failed to build request");

    //     let response = handler
    //         .serve(ImmediateReturnApp {}, request, build_conn_info(), MockState {})
    //         .await;

    //     assert!(response.is_err_and(|e| { e.to_string() == "Application is not running" }));
    // }

    // #[tokio::test]
    // async fn test_app_fails_when_called() {
    //     let handler = HTTPHandler::new(10);
    //     let request = Request::builder()
    //         .body("hello world".to_string())
    //         .expect("Failed to build request");

    //     let response = handler
    //         .serve(ErrorOnCallApp {}, request, build_conn_info(), MockState {})
    //         .await;

    //     assert!(response.is_err_and(|e| { e.to_string() == "Application error: TestError(\"Immediate error\")" }));
    // }

    // #[tokio::test]
    // async fn test_app_raises_error_while_communicating() {
    //     let handler = HTTPHandler::new(10);
    //     let request = Request::builder()
    //         .body("hello world".to_string())
    //         .expect("Failed to build request");

    //     let response = handler
    //         .serve(ErrorInLoopApp {}, request, build_conn_info(), MockState {})
    //         .await;

    //     assert!(response.is_err_and(|e| { e.to_string() == "Application error: TestError(\"Error in loop\")" }));
    // }

    // #[tokio::test]
    // async fn test_app_raises_error_while_sending_body() {
    //     let handler = HTTPHandler::new(10);
    //     let request = Request::builder()
    //         .body("hello world".to_string())
    //         .expect("Failed to build request");

    //     let response = handler
    //         .serve(ErrorInBodyLoopApp {}, request, build_conn_info(), MockState {})
    //         .await;

    //     assert!(response.is_err_and(|e| { e.to_string() == "Application error: TestError(\"Error in loop\")" }))
    // }

    // #[tokio::test]
    // async fn test_error_while_streaming_body() {
    //     let handler = HTTPHandler::new(10);
    //     let request = Request::builder()
    //         .body("hello world".to_string())
    //         .expect("Failed to build request");

    //     let response = handler
    //         .serve(ErrorInDataStreamApp {}, request, build_conn_info(), MockState {})
    //         .await;

    //     let body = response.unwrap().into_body().collect().await;

    //     assert!(body.is_err_and(|e| {
    //         e.to_string() == "Unexpected ASGI message received. StartupComplete(LifespanStartupCompleteEvent)"
    //     }))
    // }