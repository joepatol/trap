//// Core types used in ARAS
use std::net::SocketAddr;

use bytes::Bytes;
use hyper::body::Incoming;
use http::{Request as HTTPRequest, Response as HTTPResponse};
use http_body_util::combinators::BoxBody;

use crate::error::Error;

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

/// ARAS Request type
pub type Request = HTTPRequest<Incoming>;
/// ARAS Response type
pub type Response = HTTPResponse<BoxBody<Bytes, Error>>;