use std::fs;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use aras_core::ArasError;
use asgispec::prelude::*;
use asgispec::scope::LifespanScope;

use super::{Worker, spawn_worker};
use super::SOCKET_PATH;

#[derive(Clone)]
pub(crate) struct WorkerPool {
    pub workers: Arc<Vec<Worker>>,
}

impl WorkerPool {
    pub fn initialize(num_workers: usize, worker_script: &str, python: &str, app: &str, pythonpath: &str) -> Self {
        fs::create_dir_all(SOCKET_PATH).expect("Failed to create socket dir");
        let mut workers = Vec::with_capacity(num_workers);
        for id in 0..num_workers {
            let worker = spawn_worker(id, worker_script, python, app, pythonpath);
            workers.push(worker);
        }
        Self {
            workers: Arc::new(workers),
        }
    }

    fn select_worker(&self) -> &Worker {
        self.workers
            .iter()
            .min_by_key(|worker| worker.task_count.load(Ordering::Relaxed))
            .unwrap()
    }

    async fn startup<S: State + Serialize>(&self, scope: LifespanScope<S>, send: SendFn) -> Result<(), ArasError> {
        let mut results = Vec::new();
        for worker in self.workers.iter() {
            let result = worker.startup(scope.clone()).await.unwrap();
            results.push(result);
        }

        if results.iter().all(|msg| matches!(msg, ASGISendEvent::StartupComplete(_))) {
            send(ASGISendEvent::new_startup_complete()).await.unwrap();
        } else if results.iter().any(|msg| matches!(msg, ASGISendEvent::StartupFailed(_))){
            send(ASGISendEvent::new_startup_failed("A worker failed to start".into())).await.unwrap();
        } else {
            send(ASGISendEvent::new_startup_failed("Unknown startup error".into())).await.unwrap();
        }

        Ok(())
    }

    async fn shutdown(&self, send: SendFn) -> Result<(), ArasError> {
        let mut results = Vec::new();
        for worker in self.workers.iter() {
            let result = worker.shutdown().await.unwrap();
            results.push(result);
        }

        send(ASGISendEvent::new_shutdown_complete()).await.map_err(|_| ArasError::Disconnect)
    }

    async fn lifespan_loop<S: State + Serialize>(&self, scope: LifespanScope<S>, receive: ReceiveFn, send: SendFn) -> Result<(), ArasError> {
        loop {
            let msg = receive().await;
            match msg {
                ASGIReceiveEvent::Startup(_) => self.startup(scope.clone(), send.clone()).await?,
                ASGIReceiveEvent::Shutdown(_) => return self.shutdown(send.clone()).await,
                _ => {}
            }
        }
    }
}

impl ASGIApplication for WorkerPool {
    type Error = ArasError;
    type State = String;

    async fn call(&self, scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        if let Scope::Lifespan(lifespan_scope) = scope {
            return self.lifespan_loop(lifespan_scope, receive, send).await;
        }

        let worker = self.select_worker();
        worker.call(scope, receive, send).await.map_err(|e| ArasError::custom(e.to_string()))
    }
}
