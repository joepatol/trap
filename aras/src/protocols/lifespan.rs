use std::sync::Arc;
use std::time::Duration;

use asgispec::prelude::*;
use derive_more::Constructor;
use log::{debug, error, info};

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::{ArasError, ArasResult};

#[derive(Constructor)]
pub(crate) struct LifespanHandler {
    timeout: Duration,
}

impl LifespanHandler {
    pub async fn startup<S: SendToASGIApp, R: ReceiveFromASGIApp>(
        self,
        mut send_to_app: S,
        mut receive_from_app: R,
    ) -> ArasResult<StartedLifespanHandler<S, R>> {
        info!("Application starting");

        match startup_loop(&mut send_to_app, &mut receive_from_app, self.timeout).await {
            Ok(use_lifespan) => {
                info!("Application startup complete");
                Ok(StartedLifespanHandler::new(
                    send_to_app,
                    receive_from_app,
                    use_lifespan,
                    self.timeout,
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
    timeout: Duration,
}

impl<S: SendToASGIApp, R: ReceiveFromASGIApp> StartedLifespanHandler<S, R> {
    pub async fn shutdown(&mut self) -> ArasResult<()> {
        info!("Application shutting down");

        if !self.enabled {
            info!("Lifespan protocol disabled...");
            info!("Application shutdown complete");
            return Ok(());
        };

        match shutdown_loop(&mut self.send_to, &mut self.receive_from, self.timeout).await {
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
    timeout: Duration,
) -> ArasResult<bool> {
    if let Err(e) = send_to.send(ASGIReceiveEvent::new_lifespan_startup()).await {
        debug!("Lifespan protocol appears unsupported: {e}");
        return Ok(false);
    }
    match tokio::time::timeout(timeout, receive_from.receive()).await {
        Ok(Ok(ASGISendEvent::StartupComplete(_))) => Ok(true),
        Ok(Ok(ASGISendEvent::StartupFailed(event))) => Err(ArasError::custom(event.message)),
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
    timeout: Duration,
) -> ArasResult<()> {
    send_to.send(ASGIReceiveEvent::new_lifespan_shutdown()).await?;
    match tokio::time::timeout(timeout, receive_from.receive()).await? {
        Ok(ASGISendEvent::ShutdownComplete(_)) => Ok(()),
        Ok(ASGISendEvent::ShutdownFailed(event)) => Err(ArasError::custom(event.message)),
        Ok(msg) => Err(ArasError::unexpected_asgi_message(Arc::new(msg))),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use asgispec::prelude::*;
    use crate::ArasError;
    use super::{LifespanHandler, StartedLifespanHandler};
    use crate::mocks::communication::{DeterministicReceiveFromApp, SendToAppCollector, SendToAppFail};

    #[tokio::test]
    async fn test_lifespan_startup() {
        let handler = LifespanHandler::new(Duration::from_secs(5));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_startup_complete()),
        ]);

        let result = handler.startup(send_to, receive_from).await;
        assert!(result.is_ok());
        assert!(result.unwrap().enabled);
    }

    #[tokio::test]
    async fn test_lifespan_startup_send() {
        let handler = LifespanHandler::new(Duration::from_secs(5));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_startup_complete()),
        ]);

        let _ = handler.startup(send_to.clone(), receive_from).await;
        
        let messages = send_to.get_messages().await;
        
        assert_eq!(messages.len(), 1);
        assert!(messages[0] == ASGIReceiveEvent::new_lifespan_startup());
    }

    #[tokio::test]
    async fn test_lifespan_startup_fails() {
        let handler = LifespanHandler::new(Duration::from_secs(5));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_startup_failed("Startup failed".to_string())),
        ]);

        let result = handler.startup(send_to, receive_from).await;
        assert!(result.is_err_and(|e| e.to_string() == "Startup failed"));
    }

    #[tokio::test]
    async fn test_error_on_startup() {
        let handler = LifespanHandler::new(Duration::from_secs(5));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Err(ArasError::custom("Startup failed")),
        ]);

        let result = handler.startup(send_to, receive_from).await;

        assert!(result.is_ok());
        assert!(result.unwrap().enabled == false);
    }

    #[tokio::test]
    async fn test_error_when_sending_startup() {
        let handler = LifespanHandler::new(Duration::from_secs(5));
        let send_to = SendToAppFail::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_startup_complete()),
        ]);

        let result = handler.startup(send_to, receive_from).await;

        assert!(result.is_ok());
        assert!(result.unwrap().enabled == false);
    }

    #[tokio::test]
    async fn test_lifespan_not_supported_invalid_message() {
        let handler = LifespanHandler::new(Duration::from_secs(5));
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
        let handler = LifespanHandler::new(Duration::from_millis(1));
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let result = handler.startup(send_to, receive_from).await;

        assert!(result.is_ok());
        assert!(result.unwrap().enabled == false);
    }

    #[tokio::test]
    async fn test_lifespan_shutdown_ok_if_disabled() {
        let mut handler = StartedLifespanHandler::new(
            SendToAppCollector::new(),
            DeterministicReceiveFromApp::new(vec![]),
            false,
            Duration::from_secs(5),
        );
        let result = handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown() {
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_shutdown_complete()),
        ]);
        let mut handler = StartedLifespanHandler::new(
            send_to,
            receive_from,
            true,
            Duration::from_secs(5),
        );

        let result = handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown_send() {
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_shutdown_complete()),
        ]);
        let mut handler = StartedLifespanHandler::new(
            send_to.clone(),
            receive_from,
            true,
            Duration::from_secs(5),
        );

        let _ = handler.shutdown().await;

        let messages = send_to.get_messages().await;
        
        assert_eq!(messages.len(), 1);
        assert!(messages[0] == ASGIReceiveEvent::new_lifespan_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_fails() {
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_shutdown_failed("it failed".into())),
        ]);
        let mut handler = StartedLifespanHandler::new(
            send_to,
            receive_from,
            true,
            Duration::from_secs(5),
        );

        let result = handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "it failed"));
    }

    #[tokio::test]
    async fn test_error_on_shutdown() {
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Err(ArasError::custom("error during shutdown")),
        ]);
        let mut handler = StartedLifespanHandler::new(
            send_to,
            receive_from,
            true,
            Duration::from_secs(5),
        );

        let result = handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "error during shutdown"));
    }

    #[tokio::test]
    async fn test_error_when_sending_shutdown() {
        let send_to = SendToAppFail::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_shutdown_complete()),
        ]);
        let mut handler = StartedLifespanHandler::new(
            send_to,
            receive_from,
            true,
            Duration::from_secs(5),
        );

        let result = handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "SendToAppFail always fails"));
    }

    #[tokio::test]
    async fn test_shutdown_timeout() {
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);
        let mut handler = StartedLifespanHandler::new(
            send_to,
            receive_from,
            true,
            Duration::from_millis(1),
        );

        let result = handler.shutdown().await;

        assert!(result.is_err_and(|e| e.to_string() == "ASGI await timeout elapsed"));
    }
}