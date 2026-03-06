// ASGI Applications used for testing
use asgispec::prelude::*;
use asgispec::scope::LifespanScope;
use derive_more::derive::Constructor;

// Mocks
#[derive(Debug)]
pub struct TestError(String);

impl From<&str> for TestError {
    fn from(value: &str) -> Self {
        Self {
            0: value.to_string(),
        }
    }
}

impl From<String> for TestError {
    fn from(value: String) -> Self {
        Self { 0: value }
    }
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        Ok(())
    }
}

impl From<Box<dyn std::error::Error>> for TestError {
    fn from(value: Box<dyn std::error::Error>) -> Self {
        Self {
            0: value.to_string(),
        }
    }
}

impl std::error::Error for TestError {}

#[derive(Clone, Debug, Constructor)]
pub struct MockState;
impl State for MockState {}
impl std::fmt::Display for MockState {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

#[derive(Clone, Constructor)]
pub struct LifespanProtocolApp;

impl LifespanProtocolApp {
    async fn run(
        &self,
        _: LifespanScope<MockState>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> std::result::Result<(), TestError> {
        let mut quit;
        loop {
            // Receive message and get the response
            let received_message = self.receive_message(&receive).await;
            let response = self.get_response(&received_message);

            // If shutdown complete, quit
            if let ASGISendEvent::ShutdownComplete(_) = response {
                quit = true;
            } else {
                quit = false;
            }

            // Send response
            if let Err(e) = (send)(response).await {
                return Err(TestError::from(format!("Failed to send message: {e}")));
            };

            if quit {
                break;
            }
        }

        Ok(())
    }

    async fn receive_message(&self, receive: &ReceiveFn) -> ASGIReceiveEvent {
        (receive)().await
    }

    fn get_response(&self, msg: &ASGIReceiveEvent) -> ASGISendEvent {
        match msg {
            ASGIReceiveEvent::Startup(_) => ASGISendEvent::new_startup_complete(),
            ASGIReceiveEvent::Shutdown(_) => ASGISendEvent::new_shutdown_complete(),
            other => panic!("Unexpected message received {other:?}"),
        }
    }
}

impl ASGIApplication for LifespanProtocolApp {
    type Error = TestError;
    type State = MockState;

    async fn call(
        &self,
        scope: Scope<Self::State>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> std::result::Result<(), Self::Error> {
        match scope {
            Scope::Lifespan(internal) => self.run(internal, receive, send).await,
            _ => Err(TestError::from("Invalid scope provided")),
        }
    }
}

#[derive(Clone, Debug, Constructor)]
pub struct ImmediateReturnApp;

impl ASGIApplication for ImmediateReturnApp {
    type Error = TestError;
    type State = MockState;

    async fn call(
        &self,
        _scope: Scope<MockState>,
        _receive: ReceiveFn,
        _send: SendFn,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Debug, Constructor)]
pub struct ImmediateErrorApp;

impl ASGIApplication for ImmediateErrorApp {
    type Error = TestError;
    type State = MockState;

    async fn call(
        &self,
        _scope: Scope<Self::State>,
        _receive: ReceiveFn,
        _send: SendFn,
    ) -> Result<(), Self::Error> {
        Err(TestError {
            0: "Immediate error".into(),
        })
    }
}

#[derive(Clone, Debug, Constructor)]
pub struct ErrorApp;

impl ASGIApplication for ErrorApp {
    type Error = TestError;
    type State = MockState;

    async fn call(
        &self,
        _scope: Scope<Self::State>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> Result<(), Self::Error> {
        _ = (receive)().await;
        _ = (send)(ASGISendEvent::new_startup_complete()).await;
        Err(TestError { 0: "Oops".into() })
    }
}
