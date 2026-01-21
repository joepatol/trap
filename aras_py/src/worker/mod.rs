mod worker;
mod pool;

pub(crate) const SOCKET_PATH: &str = "/tmp/aras/";

pub(crate) fn get_socket_path(worker_id: usize) -> String {
    format!("{SOCKET_PATH}worker-{worker_id}.sock")
}

pub(crate) use worker::{Worker, spawn_worker};
pub(crate) use pool::WorkerPool;
