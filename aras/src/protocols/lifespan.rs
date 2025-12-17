use std::time::Duration;

use asgispec::prelude::*;
use asgispec::scope::LifespanScope;
use derive_more::Constructor;
use log::{error, info, debug};

use crate::application::{ApplicationWrapper, CalledApplication};
use crate::error::{Error, Result};

#[derive(Constructor)]
pub(crate) struct LifespanHandler {
    timeout_secs: u64,
}

impl LifespanHandler {
    pub async fn startup<A>(self, application: A, state: A::State) -> Result<StartedLifespanHandler>
    where
        A: ASGIApplication + 'static,
    {
        info!("Application starting");

        let state_clone = state.clone();
        let application = ApplicationWrapper::from(&application);
        let mut called_app = application.call(Scope::Lifespan(LifespanScope::new(
            ASGIScope::default(),
            Some(state_clone),
        )));

        match startup_loop(&mut called_app, self.timeout_secs).await {
            Ok(use_lifespan) => {
                info!("Application startup complete");
                Ok(StartedLifespanHandler::new(called_app, use_lifespan, self.timeout_secs))
            }
            Err(e) => {
                info!("Application startup failed");
                Err(e)
            }
        }
    }
}

#[derive(Constructor)]
pub struct StartedLifespanHandler {
    application: CalledApplication,
    enabled: bool,
    timeout_secs: u64,
}

impl StartedLifespanHandler {
    pub async fn shutdown(self) -> Result<()> {
        info!("Application shutting down");
        if !self.enabled {
            info!("Lifespan protocol disabled...");
            info!("Application shutdown complete");
            return Ok(());
        };

        match shutdown_loop(self.application, self.timeout_secs).await {
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

async fn startup_loop(application: &mut CalledApplication, timeout_secs: u64) -> Result<bool> {
    if let Err(e) = application.send_to(ASGIReceiveEvent::new_lifespan_startup()).await {
        debug!("Lifespan protocol appears unsupported: {e}");
        return Ok(false)
    }
    match tokio::time::timeout(Duration::from_secs(timeout_secs), application.receive_from()).await? {
        Ok(ASGISendEvent::StartupComplete(_)) => Ok(true),
        Ok(ASGISendEvent::StartupFailed(event)) => Err(Error::custom(event.message)),
        Ok(_) => {
            info!("Lifespan protocol appears unsupported (no lifespan event received)");
            Ok(false)
        },
        Err(_) => {
            info!("Lifespan protocol appears unsupported (error received)");
            Ok(false)
        }
    }
}

async fn shutdown_loop(mut application: CalledApplication, timeout_secs: u64) -> Result<()> {
    application.send_to(ASGIReceiveEvent::new_lifespan_shutdown()).await?;
    match tokio::time::timeout(Duration::from_secs(timeout_secs), application.receive_from()).await? {
        Ok(ASGISendEvent::ShutdownComplete(_)) => Ok(()),
        Ok(ASGISendEvent::ShutdownFailed(event)) => Err(Error::custom(event.message)),
        Ok(msg) => Err(Error::unexpected_asgi_message(Box::new(msg))),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use asgispec::prelude::*;
    use asgispec::scope::LifespanScope;

    use crate::applications::*;

    use super::ApplicationWrapper;
    use super::{LifespanHandler, StartedLifespanHandler};
    
    pub fn build_lifespan_scope() -> Scope<MockState> {
        Scope::Lifespan(LifespanScope::new(ASGIScope::default(), Some(MockState {})))
    }

    #[tokio::test]
    async fn test_lifespan_startup() {
        let handler = LifespanHandler::new(5);
        let result = handler.startup(LifespanApp {}, MockState {}).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown_ok_if_disabled() {
        let app = LifespanApp {};
        let running_app = ApplicationWrapper::from(&app)
            .call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(running_app, false, 5);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown() {
        let running_app = ApplicationWrapper::from(LifespanApp {})
            .call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(running_app, true, 5);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_disabled_if_protocol_unsupported() {
        let handler = LifespanHandler::new(5);
        let lifespan_handler = handler.startup(LifespanUnsupportedApp {}, MockState {}).await.unwrap();
        assert!(lifespan_handler.enabled == false);
    }

    #[tokio::test]
    async fn test_error_on_startup() {
        let handler = LifespanHandler::new(5);
        let result = handler.startup(ErrorInLoopApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during startup"));
    }

    #[tokio::test]
    async fn test_startup_fails() {
        let handler = LifespanHandler::new(5);
        let result = handler.startup(LifespanFailedApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "test"));
    }

    #[tokio::test]
    async fn test_app_fails_when_called() {
        let handler = LifespanHandler::new(5);
        let result = handler.startup(ErrorOnCallApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during startup"));
    }

    #[tokio::test]
    async fn test_app_returns_early() {
        let handler = LifespanHandler::new(5);
        let result = handler.startup(ImmediateReturnApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| {
            println!("{}", e.to_string());
            true
        }));
    }

    #[tokio::test]
    async fn test_shutdown_fails() {
        let app = ApplicationWrapper::from(LifespanFailedApp {}).call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(app, true, 5);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "test"));
    }

    #[tokio::test]
    async fn test_error_on_shutdown() {
        let app = ApplicationWrapper::from(ErrorOnCallApp {}).call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(app, true, 5);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during shutdown"));
    }
}
