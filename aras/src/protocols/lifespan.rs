use asgispec::prelude::*;
use asgispec::scope::LifespanScope;
use derive_more::Constructor;
use log::{error, info, warn};

use crate::application::{ApplicationWrapper, CalledApplication};
use crate::error::{Error, Result, UnexpectedShutdownSrc as SRC};

#[derive(Constructor)]
pub(crate) struct LifespanHandler;

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
        let result = startup_loop(&mut called_app).await;

        match result {
            Ok(use_lifespan) => {
                info!("Application startup complete");
                Ok(StartedLifespanHandler::new(called_app, use_lifespan))
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
}

impl StartedLifespanHandler {
    pub async fn shutdown(self) -> Result<()> {
        info!("Application shutting down");
        if !self.enabled {
            info!("Lifespan protocol disabled...");
            return Ok(());
        };

        match shutdown_loop(self.application).await {
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

async fn startup_loop(application: &mut CalledApplication) -> Result<bool> {
    application.send_to(ASGIReceiveEvent::new_lifespan_startup()).await?;
    match application.receive_from().await {
        Ok(ASGISendEvent::StartupComplete(_)) => Ok(true),
        Ok(ASGISendEvent::StartupFailed(event)) => Err(Error::custom(event.message)),
        _ => {
            warn!("Lifespan protocol appears unsupported");
            Ok(false)
        }
    }
}

async fn shutdown_loop(mut application: CalledApplication) -> Result<()> {
    application.send_to(ASGIReceiveEvent::new_lifespan_shutdown()).await?;
    match application.receive_from().await {
        Ok(ASGISendEvent::ShutdownComplete(_)) => Ok(()),
        Ok(ASGISendEvent::ShutdownFailed(event)) => Err(Error::custom(event.message)),
        Ok(msg) => Err(Error::unexpected_asgi_message(Box::new(msg))),
        Err(e) => Err(Error::unexpected_shutdown(
            SRC::Application,
            format!("{e}").into(),
        )),
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
        let handler = LifespanHandler::new();
        let result = handler.startup(LifespanApp {}, MockState {}).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown_ok_if_disabled() {
        let app = LifespanApp {};
        let running_app = ApplicationWrapper::from(&app)
            .call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(running_app, false);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown() {
        let running_app = ApplicationWrapper::from(LifespanApp {})
            .call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(running_app, true);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_disabled_if_protocol_unsupported() {
        let handler = LifespanHandler::new();
        let lifespan_handler = handler.startup(LifespanUnsupportedApp {}, MockState {}).await.unwrap();
        assert!(lifespan_handler.enabled == false);
    }

    #[tokio::test]
    async fn test_error_on_startup() {
        let handler = LifespanHandler::new();
        let result = handler.startup(ErrorInLoopApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during startup"));
    }

    #[tokio::test]
    async fn test_startup_fails() {
        let handler = LifespanHandler::new();
        let result = handler.startup(LifespanFailedApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "test"));
    }

    #[tokio::test]
    async fn test_app_fails_when_called() {
        let handler = LifespanHandler::new();
        let result = handler.startup(ErrorOnCallApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during startup"));
    }

    #[tokio::test]
    async fn test_app_returns_early() {
        let handler = LifespanHandler::new();
        let result = handler.startup(ImmediateReturnApp {}, MockState {}).await;
        assert!(result.is_err_and(|e| {
            println!("{}", e.to_string());
            true
        }));
    }

    #[tokio::test]
    async fn test_shutdown_fails() {
        let app = ApplicationWrapper::from(LifespanFailedApp {}).call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(app, true);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "test"));
    }

    #[tokio::test]
    async fn test_error_on_shutdown() {
        let app = ApplicationWrapper::from(ErrorOnCallApp {}).call(build_lifespan_scope());
        let lifespan_handler = StartedLifespanHandler::new(app, true);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during shutdown"));
    }
}
