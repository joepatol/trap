use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use aras_core::ArasError;
use asgispec::prelude::*;

use super::SOCKET_PATH;
use super::Worker;

pub(crate) fn create_socket_dir() {
    fs::create_dir_all(SOCKET_PATH).expect("Failed to create socket dir");
}

#[derive(Clone)]
pub(crate) struct WorkerPool {
    pub workers: Arc<Vec<Worker>>,
}

impl WorkerPool {
    pub fn initialize(num_workers: usize, worker_script: &str, python: &str, app: &str) -> Self {
        create_socket_dir();
        let mut workers = Vec::with_capacity(num_workers);
        for id in 0..num_workers {
            let worker = Worker::start(id, worker_script, python, app);
            workers.push(worker);
        }
        Self { workers: Arc::new(workers) }
    }

    fn select_worker(&self) -> &Worker {
        self.workers
            .iter()
            .min_by_key(|worker| worker.task_count.load(Ordering::Relaxed))
            .unwrap()
    }
}

impl ASGIApplication for WorkerPool {
    type Error = ArasError;
    type State = String;

    async fn call(
        &self,
        scope: Scope<Self::State>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> Result<(), Self::Error> {
        let worker = self.select_worker();
        worker.call(scope, receive, send).await.unwrap();
        Ok(())
    }
}