use std::net::SocketAddrV4;

use crate::types::ConnectionInfo;

pub fn build_conn_info() -> ConnectionInfo {
    ConnectionInfo::new(
        std::net::SocketAddr::V4(SocketAddrV4::new([127, 0, 0, 1].into(), 2)), 
        std::net::SocketAddr::V4(SocketAddrV4::new([0, 0, 0, 0].into(), 80)), 
    )
}
