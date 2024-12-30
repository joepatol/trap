use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;

use crate::error::Result;
use crate::http::*;
use crate::lifespan::*;
use crate::websocket::*;

pub const ASGI_VERSION: &str = "3.0";
pub const ASGI_SPEC_VERSION: &str = "2.4";

pub type SendFn = Arc<dyn Fn(ASGISendEvent) -> Box<dyn Future<Output = Result<()>> + Unpin + Sync + Send> + Send + Sync>;

pub type ReceiveFn = Arc<dyn Fn() -> Box<dyn Future<Output = ASGIReceiveEvent> + Unpin + Sync + Send> + Send + Sync>;

pub trait State: Clone + Send + Sync + Debug {}

pub trait ASGICallable<S: State>: Send + Sync + Clone {
    fn call(&self, scope: Scope<S>, receive: ReceiveFn, send: SendFn) -> impl Future<Output = Result<()>> + Send + Sync;
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
            Scope::Websocket(s) => write!(f, "{:?}", s),
            Scope::Lifespan(s) => write!(f, "{:?}", s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ASGIScope {
    pub version: String,
    pub spec_version: String,
}

impl ASGIScope {
    pub fn new() -> Self {
        Self {
            version: ASGI_VERSION.to_string(),
            spec_version: ASGI_SPEC_VERSION.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ASGISendEvent {
    StartupComplete(LifespanStartupComplete),
    StartupFailed(LifespanStartupFailed),
    ShutdownComplete(LifespanShutdownComplete),
    ShutdownFailed(LifespanShutdownFailed),
    HTTPResponseStart(HTTPResponseStartEvent),
    HTTPResponseBody(HTTPResonseBodyEvent),
    WebsocketAccept(WebsocketAcceptEvent),
    WebsocketClose(WebsocketCloseEvent),
    WebsocketSend(WebsocketSendEvent),
}

#[derive(Debug, Clone)]
pub enum ASGIReceiveEvent {
    Startup(LifespanStartup),
    Shutdown(LifespanShutdown),
    HTTPRequest(HTTPRequestEvent),
    HTTPDisconnect(HTTPDisconnectEvent),
    WebsocketConnect(WebsocketConnectEvent),
    WebsocketDisconnect(WebsocketDisconnectEvent),
    WebsocketReceive(WebsocketReceiveEvent),
}

impl ASGISendEvent {
    pub fn new_startup_complete() -> Self {
        Self::StartupComplete(LifespanStartupComplete::new())
    }

    pub fn new_startup_failed(message: String) -> Self {
        Self::StartupFailed(LifespanStartupFailed::new(message))
    }

    pub fn new_shutdown_complete() -> Self {
        Self::ShutdownComplete(LifespanShutdownComplete::new())
    }

    pub fn new_shutdown_failed(message: String) -> Self {
        Self::ShutdownFailed(LifespanShutdownFailed::new(message))
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
        Self::Startup(LifespanStartup::new())
    }

    pub fn new_lifespan_shutdown() -> Self {
        Self::Shutdown(LifespanShutdown::new())
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
            Self::StartupComplete(s) => write!(f, "{:?}", s),
            Self::StartupFailed(s) => write!(f, "{:?}", s),
            Self::ShutdownComplete(s) => write!(f, "{:?}", s),
            Self::ShutdownFailed(s) => write!(f, "{:?}", s),
            Self::HTTPResponseStart(s) => write!(f, "{:?}", s),
            Self::HTTPResponseBody(s) => write!(f, "{}", s),
            Self::WebsocketAccept(s) => write!(f, "{:?}", s),
            Self::WebsocketClose(s) => write!(f, "{:?}", s),
            Self::WebsocketSend(s) => write!(f, "{:?}", s),
        }
    }
}

impl std::fmt::Display for ASGIReceiveEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Startup(s) => write!(f, "{:?}", s),
            Self::Shutdown(s) => write!(f, "{:?}", s),
            Self::HTTPRequest(s) => write!(f, "{}", s),
            Self::HTTPDisconnect(s) => write!(f, "{:?}", s),
            Self::WebsocketConnect(s) => write!(f, "{:?}", s),
            Self::WebsocketReceive(s) => write!(f, "{:?}", s),
            Self::WebsocketDisconnect(s) => write!(f, "{:?}", s),
        }
    }
}
