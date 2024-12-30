use derive_more::Constructor;
use log::{error, info, warn};

use crate::application::{Application, RunningApplication};
use crate::asgispec::{ASGICallable, ASGIReceiveEvent, ASGISendEvent, Scope, State};
use crate::error::{Error, Result};

use super::LifespanScope;

#[derive(Constructor)]
pub struct LifespanHandler<S: State + 'static, T: ASGICallable<S> + 'static> {
    application: Application<S, T>,
}

impl<S, T> LifespanHandler<S, T>
where
    S: State,
    T: ASGICallable<S>,
{
    pub async fn startup(self, state: S) -> Result<StartedLifespanHandler> {
        info!("Application starting");

        let app_clone = self.application.clone();
        let state_clone = state.clone();

        let called_app = app_clone.call(Scope::Lifespan(LifespanScope::new(state_clone))).0;
        let result = startup_loop(called_app.clone()).await;

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
    application: RunningApplication,
    enabled: bool,
}

impl StartedLifespanHandler {
    pub async fn shutdown(self) -> Result<()> {
        info!("Application shutting down");
        if !self.enabled {
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

async fn startup_loop(mut application: RunningApplication) -> Result<bool> {
    application.send_to(ASGIReceiveEvent::new_lifespan_startup()).await?;
    match application.receive_from().await {
        Some(ASGISendEvent::StartupComplete(_)) => Ok(true),
        Some(ASGISendEvent::StartupFailed(event)) => Err(Error::custom(event.message)),
        None => Err(Error::unexpected_shutdown(
            "Application",
            "stopped during startup".into(),
        )),
        _ => {
            warn!("Lifespan protocol appears unsupported");
            Ok(false)
        }
    }
}

async fn shutdown_loop(mut application: RunningApplication) -> Result<()> {
    application.send_to(ASGIReceiveEvent::new_lifespan_shutdown()).await?;
    match application.receive_from().await {
        Some(ASGISendEvent::ShutdownComplete(_)) => Ok(()),
        Some(ASGISendEvent::ShutdownFailed(event)) => Err(Error::custom(event.message)),
        Some(msg) => Err(Error::unexpected_asgi_message(Box::new(msg))),
        None => Err(Error::unexpected_shutdown(
            "Application",
            "stopped during shutdown".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{LifespanHandler, StartedLifespanHandler};
    use crate::application::ApplicationFactory;
    use crate::asgispec::{ASGICallable, ASGIReceiveEvent, ASGISendEvent, ReceiveFn, Scope, SendFn, State};
    use crate::error::Error;

    #[derive(Clone, Debug)]
    struct MockState;
    impl State for MockState {}

    #[derive(Clone, Debug)]
    struct LifespanApp;

    impl ASGICallable<MockState> for LifespanApp {
        async fn call(&self, scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> super::Result<()> {
            if let Scope::Lifespan(_) = scope {
                loop {
                    match receive().await {
                        ASGIReceiveEvent::Startup(_) => {
                            send(ASGISendEvent::new_startup_complete()).await?;
                        }
                        ASGIReceiveEvent::Shutdown(_) => return send(ASGISendEvent::new_shutdown_complete()).await,
                        _ => return Err(Error::custom("Invalid message")),
                    }
                }
            };
            Err(Error::custom("Invalid scope"))
        }
    }

    #[derive(Clone, Debug)]
    struct LifespanUnsupportedApp;

    impl ASGICallable<MockState> for LifespanUnsupportedApp {
        async fn call(&self, scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> super::Result<()> {
            if let Scope::Lifespan(_) = scope {
                loop {
                    _ = receive().await;
                    // Send an unrelated message, to mimick the protocol not being supported
                    send(ASGISendEvent::new_http_response_body("oops".into(), false)).await?;
                }
            };
            Err(Error::custom("Invalid scope"))
        }
    }

    #[derive(Clone, Debug)]
    struct ErrorApp;

    impl ASGICallable<MockState> for ErrorApp {
        async fn call(&self, _scope: Scope<MockState>, receive: ReceiveFn, _send: SendFn) -> super::Result<()> {
            _ = receive().await;
            Err(Error::custom("Test app raises error"))
        }
    }

    #[derive(Clone, Debug)]
    struct ErrorOnCallApp;

    impl ASGICallable<MockState> for ErrorOnCallApp {
        async fn call(&self, _scope: Scope<MockState>, _receive: ReceiveFn, _send: SendFn) -> super::Result<()> {
            Err(Error::custom("Immediate error"))
        }
    }

    #[derive(Clone, Debug)]
    struct ImmediateReturnApp;

    impl ASGICallable<MockState> for ImmediateReturnApp {
        async fn call(&self, _scope: Scope<MockState>, _receive: ReceiveFn, _send: SendFn) -> super::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    struct LifespanFailedApp;

    impl ASGICallable<MockState> for LifespanFailedApp {
        async fn call(&self, scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> super::Result<()> {
            if let Scope::Lifespan(_) = scope {
                loop {
                    match receive().await {
                        ASGIReceiveEvent::Startup(_) => {
                            send(ASGISendEvent::new_startup_failed("test".to_string())).await?;
                        }
                        ASGIReceiveEvent::Shutdown(_) => {
                            return send(ASGISendEvent::new_shutdown_failed("test".to_string())).await
                        }
                        _ => return Err(Error::custom("Invalid message")),
                    }
                }
            };
            Err(Error::custom("Invalid scope"))
        }
    }

    #[tokio::test]
    async fn test_lifespan_startup() {
        let app = ApplicationFactory::new(LifespanApp {}).build();
        let lifespan_handler = LifespanHandler::new(app);
        let result = lifespan_handler.startup(MockState {}).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown_ok_if_disabled() {
        let app = ApplicationFactory::new(LifespanApp {})
            .build()
            .call(Scope::Lifespan(super::LifespanScope::new(MockState {})))
            .0;
        let lifespan_handler = StartedLifespanHandler::new(app.clone(), false);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_shutdown() {
        let app = ApplicationFactory::new(LifespanApp {})
            .build()
            .call(Scope::Lifespan(super::LifespanScope::new(MockState {})))
            .0;
        let lifespan_handler = StartedLifespanHandler::new(app.clone(), true);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifespan_disabled_if_protocol_unsupported() {
        let app = ApplicationFactory::new(LifespanUnsupportedApp {}).build();
        let lifespan_handler = LifespanHandler::new(app);
        let lifespan_handler = lifespan_handler.startup(MockState {}).await.unwrap();
        assert!(lifespan_handler.enabled == false);
    }

    #[tokio::test]
    async fn test_error_on_startup() {
        let app = ApplicationFactory::new(ErrorApp {}).build();
        let lifespan_handler = LifespanHandler::new(app);
        let result = lifespan_handler.startup(MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during startup"));
    }

    #[tokio::test]
    async fn test_startup_fails() {
        let app = ApplicationFactory::new(LifespanFailedApp {}).build();
        let lifespan_handler = LifespanHandler::new(app);
        let result = lifespan_handler.startup(MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "test"));
    }

    #[tokio::test]
    async fn test_app_fails_when_called() {
        let app = ApplicationFactory::new(ErrorOnCallApp {}).build();
        let lifespan_handler = LifespanHandler::new(app);
        let result = lifespan_handler.startup(MockState {}).await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during startup"));
    }

    #[tokio::test]
    async fn test_app_returns_early() {
        let app = ApplicationFactory::new(ImmediateReturnApp {}).build();
        let lifespan_handler = LifespanHandler::new(app);
        let result = lifespan_handler.startup(MockState {}).await;
        assert!(result.is_err_and(|e| {
            println!("{}", e.to_string());
            true
        }));
    }

    #[tokio::test]
    async fn test_shutdown_fails() {
        let app = ApplicationFactory::new(LifespanFailedApp {})
            .build()
            .call(Scope::Lifespan(super::LifespanScope::new(MockState {})))
            .0;
        let lifespan_handler = StartedLifespanHandler::new(app.clone(), true);
        let result = lifespan_handler.shutdown().await;
        println!("{result:?}");
        assert!(result.is_err_and(|e| e.to_string() == "test"));
    }

    #[tokio::test]
    async fn test_error_on_shutdown() {
        let app = ApplicationFactory::new(ErrorApp {})
            .build()
            .call(Scope::Lifespan(super::LifespanScope::new(MockState {})))
            .0;
        let lifespan_handler = StartedLifespanHandler::new(app.clone(), true);
        let result = lifespan_handler.shutdown().await;
        assert!(result.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped during shutdown"));
    }
}
