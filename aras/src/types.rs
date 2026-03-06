use std::future::Future;
/// Core types used in ARAS
use std::net::SocketAddr;
use std::pin::Pin;

use bytes::Bytes;
use http::Request as HTTPRequest;
use http::Response as HTTPResponse;
use http_body_util::combinators::BoxBody;
use hyper::body::Incoming;

use crate::{ArasError, ArasResult};

/// Data structure containing information on the current connection
#[derive(Clone, Debug)]
pub struct ConnectionInfo {
    pub client_ip: String,
    pub server_ip: String,
    pub client_port: u16,
    pub server_port: u16,
}

impl ConnectionInfo {
    pub fn new(client: SocketAddr, server: SocketAddr) -> Self {
        Self {
            client_ip: client.ip().to_string(),
            server_ip: server.ip().to_string(),
            client_port: client.port(),
            server_port: server.port(),
        }
    }
}

pub trait ResponseStatus {
    fn status_string(&self) -> String;
}

/// ARAS Request type
pub type Request = HTTPRequest<Incoming>;
/// ARAS Response type
pub type Response = HTTPResponse<BoxBody<Bytes, ArasError>>;
/// Future type for all services
pub type ServiceFuture = Pin<Box<dyn Future<Output = ArasResult<Response>> + Send>>;

impl<T> ResponseStatus for HTTPResponse<T> {
    fn status_string(&self) -> String {
        format!(
            "{} {}",
            self.status().as_str().to_owned(),
            self.status().canonical_reason().unwrap_or("")
        )
    }
}
