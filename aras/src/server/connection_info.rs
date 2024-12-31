use std::net::SocketAddr;

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