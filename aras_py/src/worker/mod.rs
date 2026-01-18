mod worker;
mod pool;

pub(crate) const SOCKET_PATH: &str = "/tmp/aras/";

pub(crate) use worker::Worker;
