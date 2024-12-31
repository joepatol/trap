use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::events::*;
use crate::scope::*;

pub const ASGI_VERSION: &str = "3.0";
pub const ASGI_SPEC_VERSION: &str = "2.4";

pub type ASGIResult<T> = std::result::Result<T, Arc<dyn Error + Send + Sync>>;

pub type SendFn =
    Arc<dyn Fn(ASGISendEvent) -> Box<dyn Future<Output = ASGIResult<()>> + Unpin + Sync + Send> + Send + Sync>;

pub type ReceiveFn = Arc<dyn Fn() -> Box<dyn Future<Output = ASGIReceiveEvent> + Unpin + Sync + Send> + Send + Sync>;

pub trait State: Clone + Send + Sync + Display {}

pub trait ASGIApplication<S: State>: Send + Sync + Clone {
    fn call(&self, scope: Scope<S>, receive: ReceiveFn, send: SendFn) -> impl Future<Output = ASGIResult<()>> + Send;
}

pub trait ASGIServer<S: State> {
    fn serve(&self, application: impl ASGIApplication<S>, state: S) -> impl Future<Output = ASGIResult<Sender<()>>> + Send;
}

#[derive(Debug, Clone)]
pub enum Scope<S: State> {
    HTTP(HTTPScope<S>),
    Lifespan(LifespanScope<S>),
    Websocket(WebsocketScope<S>),
}

impl<S: State> std::fmt::Display for Scope<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::HTTP(s) => write!(f, "{}", s),
            Scope::Websocket(s) => write!(f, "{}", s),
            Scope::Lifespan(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ASGIScope {
    pub version: String,
    pub spec_version: String,
}

impl ASGIScope {
    pub fn new(version: String, spec_version: String) -> Self {
        Self { version, spec_version }
    }
}

impl Default for ASGIScope {
    fn default() -> Self {
        Self {
            version: ASGI_VERSION.to_string(),
            spec_version: ASGI_SPEC_VERSION.to_string(),
        }
    }
}

impl std::fmt::Display for ASGIScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "version: {}", self.version)?;
        writeln!(f, "spec version: {}", self.spec_version)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ASGISendEvent {
    StartupComplete(LifespanStartupCompleteEvent),
    StartupFailed(LifespanStartupFailedEvent),
    ShutdownComplete(LifespanShutdownCompleteEvent),
    ShutdownFailed(LifespanShutdownFailedEvent),
    HTTPResponseStart(HTTPResponseStartEvent),
    HTTPResponseBody(HTTPResonseBodyEvent),
    WebsocketAccept(WebsocketAcceptEvent),
    WebsocketClose(WebsocketCloseEvent),
    WebsocketSend(WebsocketSendEvent),
}

#[derive(Debug, Clone)]
pub enum ASGIReceiveEvent {
    Startup(LifespanStartupEvent),
    Shutdown(LifespanShutdownEvent),
    HTTPRequest(HTTPRequestEvent),
    HTTPDisconnect(HTTPDisconnectEvent),
    WebsocketConnect(WebsocketConnectEvent),
    WebsocketDisconnect(WebsocketDisconnectEvent),
    WebsocketReceive(WebsocketReceiveEvent),
}

impl ASGISendEvent {
    pub fn new_startup_complete() -> Self {
        Self::StartupComplete(LifespanStartupCompleteEvent::new())
    }

    pub fn new_startup_failed(message: String) -> Self {
        Self::StartupFailed(LifespanStartupFailedEvent::new(message))
    }

    pub fn new_shutdown_complete() -> Self {
        Self::ShutdownComplete(LifespanShutdownCompleteEvent::new())
    }

    pub fn new_shutdown_failed(message: String) -> Self {
        Self::ShutdownFailed(LifespanShutdownFailedEvent::new(message))
    }

    pub fn new_http_response_start(status: u16, headers: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Self::HTTPResponseStart(HTTPResponseStartEvent::new(status, headers))
    }

    pub fn new_http_response_body(data: Vec<u8>, more_body: bool) -> Self {
        Self::HTTPResponseBody(HTTPResonseBodyEvent::new(data, more_body))
    }

    pub fn new_websocket_accept(subprotocol: Option<String>, headers: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Self::WebsocketAccept(WebsocketAcceptEvent::new(subprotocol, headers))
    }

    pub fn new_websocket_close(code: Option<usize>, reason: String) -> Self {
        Self::WebsocketClose(WebsocketCloseEvent::new(code, reason))
    }

    pub fn new_websocket_send(bytes: Option<Vec<u8>>, text: Option<String>) -> Self {
        Self::WebsocketSend(WebsocketSendEvent::new(bytes, text))
    }
}

impl ASGIReceiveEvent {
    pub fn new_lifespan_startup() -> Self {
        Self::Startup(LifespanStartupEvent::new())
    }

    pub fn new_lifespan_shutdown() -> Self {
        Self::Shutdown(LifespanShutdownEvent::new())
    }

    pub fn new_http_request(data: Vec<u8>, more_body: bool) -> Self {
        Self::HTTPRequest(HTTPRequestEvent::new(data, more_body))
    }

    pub fn new_http_disconnect() -> Self {
        Self::HTTPDisconnect(HTTPDisconnectEvent::new())
    }

    pub fn new_websocket_connect() -> Self {
        Self::WebsocketConnect(WebsocketConnectEvent::new())
    }

    pub fn new_websocket_receive(bytes: Option<Vec<u8>>, text: Option<String>) -> Self {
        Self::WebsocketReceive(WebsocketReceiveEvent::new(bytes, text))
    }

    pub fn new_websocket_disconnect(code: usize) -> Self {
        Self::WebsocketDisconnect(WebsocketDisconnectEvent::new(code))
    }
}

impl std::fmt::Display for ASGISendEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StartupComplete(s) => write!(f, "{}", s),
            Self::StartupFailed(s) => write!(f, "{}", s),
            Self::ShutdownComplete(s) => write!(f, "{}", s),
            Self::ShutdownFailed(s) => write!(f, "{}", s),
            Self::HTTPResponseStart(s) => write!(f, "{}", s),
            Self::HTTPResponseBody(s) => write!(f, "{}", s),
            Self::WebsocketAccept(s) => write!(f, "{}", s),
            Self::WebsocketClose(s) => write!(f, "{}", s),
            Self::WebsocketSend(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for ASGIReceiveEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Startup(s) => write!(f, "{}", s),
            Self::Shutdown(s) => write!(f, "{}", s),
            Self::HTTPRequest(s) => write!(f, "{}", s),
            Self::HTTPDisconnect(s) => write!(f, "{}", s),
            Self::WebsocketConnect(s) => write!(f, "{}", s),
            Self::WebsocketReceive(s) => write!(f, "{}", s),
            Self::WebsocketDisconnect(s) => write!(f, "{}", s),
        }
    }
}
