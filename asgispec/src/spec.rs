use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

use bytes::Bytes;
use serde::{Serialize, Deserialize};

use crate::events::*;
use crate::scope::*;

pub const ASGI_VERSION: &str = "3.0";
pub const ASGI_SPEC_VERSION: &str = "2.5";

#[derive(Debug)]
pub struct DisconnectedClient;

impl std::fmt::Display for DisconnectedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Disconnected client")?;
        Ok(())
    }
}

impl std::error::Error for DisconnectedClient {}

pub type SendFuture = Box<dyn Future<Output = Result<(), DisconnectedClient>> + Unpin + Sync + Send>;
pub type ReceiveFuture = Box<dyn Future<Output = ASGIReceiveEvent> + Unpin + Sync + Send>;
pub type SendFn = Arc<dyn Fn(ASGISendEvent) -> SendFuture + Send + Sync>;
pub type ReceiveFn = Arc<dyn Fn() -> ReceiveFuture + Send + Sync>;

pub trait State: Clone + Send + Sync + Display {}

impl State for String {}

pub trait ASGIApplication: Send + Clone {
    type State: State;
    type Error: Error;
    fn call(
        &self,
        scope: Scope<Self::State>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub trait ASGIServer<A: ASGIApplication> {
    type Output;
    fn run(&self, application: A, state: A::State) -> impl Future<Output = Self::Output>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Scope<S: State> {
    #[serde(rename = "http")]
    HTTP(HTTPScope<S>),
    #[serde(rename = "lifespan")]
    Lifespan(LifespanScope<S>),
    #[serde(rename = "websocket")]
    Websocket(WebsocketScope<S>),
}

impl<S: State> Scope<S> {
    pub fn is_websocket(&self) -> bool {
        matches!(self, Scope::Websocket(_))
    }

    pub fn is_http(&self) -> bool {
        matches!(self, Scope::HTTP(_))
    }

    pub fn is_lifespan(&self) -> bool {
        matches!(self, Scope::Lifespan(_))
    }
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

impl<S: State> From<HTTPScope<S>> for Scope<S> {
    fn from(value: HTTPScope<S>) -> Self {
        Self::HTTP(value)
    }
}

impl<S: State> From<WebsocketScope<S>> for Scope<S> {
    fn from(value: WebsocketScope<S>) -> Self {
        Self::Websocket(value)
    }
}

impl<S: State> From<LifespanScope<S>> for Scope<S> {
    fn from(value: LifespanScope<S>) -> Self {
        Self::Lifespan(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ASGISendEvent {
    #[serde(rename = "lifespan.startup.complete")]
    StartupComplete(LifespanStartupCompleteEvent),
    #[serde(rename = "lifespan.startup.failed")]
    StartupFailed(LifespanStartupFailedEvent),
    #[serde(rename = "lifespan.shutdown.complete")]
    ShutdownComplete(LifespanShutdownCompleteEvent),
    #[serde(rename = "lifespan.shutdown.failed")]
    ShutdownFailed(LifespanShutdownFailedEvent),
    #[serde(rename = "http.response.start")]
    HTTPResponseStart(HTTPResponseStartEvent),
    #[serde(rename = "http.response.body")]
    HTTPResponseBody(HTTPResponseBodyEvent),
    #[serde(rename = "websocket.accept")]
    WebsocketAccept(WebsocketAcceptEvent),
    #[serde(rename = "websocket.close")]
    WebsocketClose(WebsocketCloseEvent),
    #[serde(rename = "websocket.send")]
    WebsocketSend(WebsocketSendEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ASGIReceiveEvent {
    #[serde(rename = "lifespan.startup")]
    Startup(LifespanStartupEvent),
    #[serde(rename = "lifespan.shutdown")]
    Shutdown(LifespanShutdownEvent),
    #[serde(rename = "http.request")]
    HTTPRequest(HTTPRequestEvent),
    #[serde(rename = "http.disconnect")]
    HTTPDisconnect(HTTPDisconnectEvent),
    #[serde(rename = "websocket.connect")]
    WebsocketConnect(WebsocketConnectEvent),
    #[serde(rename = "websocket.disconnect")]
    WebsocketDisconnect(WebsocketDisconnectEvent),
    #[serde(rename = "websocket.receive")]
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

    pub fn new_http_response_start(status: u16, headers: Vec<(Bytes, Bytes)>) -> Self {
        Self::HTTPResponseStart(HTTPResponseStartEvent::new(status, headers))
    }

    pub fn new_http_response_body(data: Bytes, more_body: bool) -> Self {
        Self::HTTPResponseBody(HTTPResponseBodyEvent::new(data, more_body))
    }

    pub fn new_websocket_accept(subprotocol: Option<String>, headers: Vec<(Bytes, Bytes)>) -> Self {
        Self::WebsocketAccept(WebsocketAcceptEvent::new(subprotocol, headers))
    }

    pub fn new_websocket_close(code: u16, reason: String) -> Self {
        Self::WebsocketClose(WebsocketCloseEvent::new(Some(code), reason))
    }

    pub fn new_websocket_send(bytes: Option<Bytes>, text: Option<String>) -> Self {
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

    pub fn new_http_request(data: Bytes, more_body: bool) -> Self {
        Self::HTTPRequest(HTTPRequestEvent::new(data, more_body))
    }

    pub fn new_http_disconnect() -> Self {
        Self::HTTPDisconnect(HTTPDisconnectEvent::new())
    }

    pub fn new_websocket_connect() -> Self {
        Self::WebsocketConnect(WebsocketConnectEvent::new())
    }

    pub fn new_websocket_receive(bytes: Option<Bytes>, text: Option<String>) -> Self {
        Self::WebsocketReceive(WebsocketReceiveEvent::new(bytes, text))
    }

    pub fn new_websocket_disconnect(code: u16, reason: String) -> Self {
        Self::WebsocketDisconnect(WebsocketDisconnectEvent::new(code, reason))
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

#[cfg(test)]
mod tests{
    use super::*;

    #[test]
    fn test_lifespan_scope_serialize_ok() {
        let scope = LifespanScope::<String>::new(
            ASGIScope::default(), Some("state".to_string())
        );
        
        let scope_enum = Scope::Lifespan(scope);
        let serialized = serde_json::to_string(&scope_enum).unwrap();

        let expected = r#"{"type":"lifespan","asgi":{"version":"3.0","spec_version":"2.5"},"state":"state"}"#;
        assert!(serialized == expected);
    }

    #[test]
    fn test_http_scope_serialize_ok() {
        let scope = HTTPScope::<String>::new(
            ASGIScope::default(),
            "http".to_string(),
            "GET".into(),
            "http".into(),
            "/".to_string(),
            b"/".to_vec(),
            vec![],
            "".to_string(),
            vec![
                ("host".as_bytes().to_vec(), "localhost".as_bytes().to_vec())
            ],
            None,
            None,
            Some("state".to_string())
        );
        
        let scope_enum = Scope::HTTP(scope);
        let serialized = serde_json::to_string(&scope_enum).unwrap();

        let expected = r#"{"type":"http","asgi":{"version":"3.0","spec_version":"2.5"},"http_version":"http","method":"GET","scheme":"http","path":"/","raw_path":[47],"query_string":[],"root_path":"","headers":[[[104,111,115,116],[108,111,99,97,108,104,111,115,116]]],"client":null,"server":null,"state":"state"}"#;
        assert!(serialized == expected);
    }
}