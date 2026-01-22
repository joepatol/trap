mod worker;
mod pool;

pub(crate) use worker::Worker;
pub(crate) use pool::WorkerPool;

pub(crate) const SOCKET_PATH: &str = "/tmp/aras/";

pub(crate) fn get_socket_path(worker_id: usize) -> String {
    format!("{SOCKET_PATH}worker-{worker_id}.sock")
}