use std::sync::Arc;
use std::time::Duration;

use asgispec::prelude::*;
use derive_more::Constructor;
use log::{debug, error, info};

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::errors::{Error, Result};

#[derive(Constructor)]
pub(crate) struct LifespanHandler {
    timeout_secs: u64,
}

impl LifespanHandler {
    pub async fn startup<S: SendToASGIApp, R: ReceiveFromASGIApp>(
        self,
        mut send_to_app: S,
        mut receive_from_app: R,
    ) -> Result<StartedLifespanHandler<S, R>> {
        info!("Application starting");

        match startup_loop(&mut send_to_app, &mut receive_from_app, self.timeout_secs).await {
            Ok(use_lifespan) => {
                info!("Application startup complete");
                Ok(StartedLifespanHandler::new(
                    send_to_app,
                    receive_from_app,
                    use_lifespan,
                    self.timeout_secs,
                ))
            }
            Err(e) => {
                info!("Application startup failed");
                Err(e)
            }
        }
    }
}

#[derive(Constructor, Debug)]
pub struct StartedLifespanHandler<S: SendToASGIApp, R: ReceiveFromASGIApp> {
    send_to: S,
    receive_from: R,
    pub(crate) enabled: bool,
    timeout_secs: u64,
}

impl<S: SendToASGIApp, R: ReceiveFromASGIApp> StartedLifespanHandler<S, R> {
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Application shutting down");

        if !self.enabled {
            info!("Lifespan protocol disabled...");
            info!("Application shutdown complete");
            return Ok(());
        };

        match shutdown_loop(&mut self.send_to, &mut self.receive_from, self.timeout_secs).await {
            Ok(_) => {
                info!("Application shutdown complete");
                Ok(())
            }
            Err(e) => {
                error!("Application shutdown failed");
                Err(e)
            }
        }
    }
}

async fn startup_loop(
    send_to: &mut impl SendToASGIApp,
    receive_from: &mut impl ReceiveFromASGIApp,
    timeout_secs: u64,
) -> Result<bool> {
    if let Err(e) = send_to.send(ASGIReceiveEvent::new_lifespan_startup()).await {
        debug!("Lifespan protocol appears unsupported: {e}");
        return Ok(false);
    }
    match tokio::time::timeout(Duration::from_secs(timeout_secs), receive_from.receive()).await {
        Ok(Ok(ASGISendEvent::StartupComplete(_))) => Ok(true),
        Ok(Ok(ASGISendEvent::StartupFailed(event))) => Err(Error::custom(event.message)),
        Ok(Ok(_)) => {
            info!("Lifespan protocol appears unsupported (no lifespan event received)");
            Ok(false)
        }
        _ => {
            info!("Lifespan protocol appears unsupported (error received)");
            Ok(false)
        }
    }
}

async fn shutdown_loop(
    send_to: &mut impl SendToASGIApp,
    receive_from: &mut impl ReceiveFromASGIApp,
    timeout_secs: u64,
) -> Result<()> {
    send_to.send(ASGIReceiveEvent::new_lifespan_shutdown()).await?;
    match tokio::time::timeout(Duration::from_secs(timeout_secs), receive_from.receive()).await? {
        Ok(ASGISendEvent::ShutdownComplete(_)) => Ok(()),
        Ok(ASGISendEvent::ShutdownFailed(event)) => Err(Error::custom(event.message)),
        Ok(msg) => Err(Error::unexpected_asgi_message(Arc::new(msg))),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use asgispec::prelude::*;
    use super::LifespanHandler;
    use crate::communication_mocks::{DeterministicReceiveFromApp, SendToAppCollector};

    #[tokio::test]
    async fn test_lifespan_startup() {
        let handler = LifespanHandler::new(5);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_startup_complete()),
        ]);

        let result = handler.startup(send_to, receive_from).await;
        assert!(result.is_ok());
        assert!(result.unwrap().enabled);
    }

    #[tokio::test]
    async fn test_lifespan_startup_fails() {
        let handler = LifespanHandler::new(5);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_startup_failed("Startup failed".to_string())),
        ]);

        let result = handler.startup(send_to, receive_from).await;
        assert!(result.is_err_and(|e| e.to_string() == "Startup failed"));
    }

    #[tokio::test]
    async fn test_lifespan_not_supported_invalid_message() {
        let handler = LifespanHandler::new(5);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_shutdown_complete()),
        ]);

        let result = handler.startup(send_to, receive_from).await;
        assert!(result.is_ok());
        assert!(result.unwrap().enabled == false);
    }

    #[tokio::test]
    async fn test_lifespan_not_supported_timeout() {
        let handler = LifespanHandler::new(1);
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let result = handler.startup(send_to, receive_from).await;

        assert!(result.is_ok());
        assert!(result.unwrap().enabled == false);
    }

    //     #[tokio::test]
//     async fn test_lifespan_shutdown_ok_if_disabled() {
//         let app = LifespanApp {};
//         let running_app = ApplicationWrapper::from(&app)
//             .call(build_lifespan_scope());
//         let lifespan_handler = StartedLifespanHandler::new(running_app, false, 5);
//         let result = lifespan_handler.shutdown().await;
//         assert!(result.is_ok());
//     }

//     #[tokio::test]
//     async fn test_lifespan_shutdown() {
//         let running_app = ApplicationWrapper::from(LifespanApp {})
//             .call(build_lifespan_scope());
//         let lifespan_handler = StartedLifespanHandler::new(running_app, true, 5);
//         let result = lifespan_handler.shutdown().await;
//         assert!(result.is_ok());
//     }

//     #[tokio::test]
//     async fn test_lifespan_disabled_if_protocol_unsupported() {
//         let handler = LifespanHandler::new(5);
//         let lifespan_handler = handler.startup(LifespanUnsupportedApp {}, MockState {}).await.unwrap();
//         assert!(lifespan_handler.enabled == false);
//     }

//     #[tokio::test]
//     async fn test_error_on_startup() {
//         let handler = LifespanHandler::new(5);
//         let result = handler.startup(ErrorInLoopApp {}, MockState {}).await;

//         assert!(result.unwrap().enabled == false);
//     }

//     #[tokio::test]
//     async fn test_startup_fails() {
//         let handler = LifespanHandler::new(5);
//         let result = handler.startup(LifespanFailedApp {}, MockState {}).await;
//         assert!(result.is_err_and(|e| e.to_string() == "test"));
//     }

//     #[tokio::test]
//     async fn test_app_fails_when_called() {
//         let handler = LifespanHandler::new(5);
//         let result = handler.startup(ErrorOnCallApp {}, MockState {}).await;

//         assert!(result.unwrap().enabled == false);
//     }

//     #[tokio::test]
//     async fn test_app_returns_early() {
//         let handler = LifespanHandler::new(5);
//         let result = handler.startup(ImmediateReturnApp {}, MockState {}).await;

//         assert!(result.unwrap().enabled == false);
//     }

//     #[tokio::test]
//     async fn test_shutdown_fails() {
//         let app = ApplicationWrapper::from(LifespanFailedApp {}).call(build_lifespan_scope());
//         let lifespan_handler = StartedLifespanHandler::new(app, true, 5);
//         let result = lifespan_handler.shutdown().await;
//         assert!(result.is_err_and(|e| e.to_string() == "test"));
//     }

//     #[tokio::test]
//     async fn test_error_on_shutdown() {
//         let app = ApplicationWrapper::from(ErrorOnCallApp {}).call(build_lifespan_scope());
//         let lifespan_handler = StartedLifespanHandler::new(app, true, 5);
//         tokio::task::yield_now().await;
//         let result = lifespan_handler.shutdown().await;

//         assert!(result.is_err_and(|e| {
//             e.to_string() == "Application error: TestError(\"Immediate error\")"
//         }));
//     }
}
