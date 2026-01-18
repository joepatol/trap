use std::path::Path;
use std::process::{Child, Command};
use std::result::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use asgispec::prelude::*;
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

use super::SOCKET_PATH;

fn get_socket_path(worker_id: usize) -> String {
    format!("{SOCKET_PATH}worker-{worker_id}.sock")
}

fn start_python_worker(id: usize, python: &str, worker_script: &str, socket_path: &str, app: &str) -> Child {
    Command::new(python)
        .arg(worker_script)
        .args(["--socket", socket_path, "--id", &id.to_string(), "--app", app])
        .spawn()
        .unwrap()
}

fn wait_for_socket(socket_path: &str) {
    let path = Path::new(socket_path);
    while !path.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) struct Worker {
    pub task_count: AtomicUsize,
    pub id: usize,
    socket_path: String,
    python_worker: Child,
}

impl Worker {
    pub fn start(id: usize, worker_script: &str, python: &str, app: &str) -> Self {
        let socket_path = get_socket_path(id);
        let _ = std::fs::remove_file(&socket_path);
        let python_worker = start_python_worker(id, python, worker_script, &socket_path, app);
        wait_for_socket(&socket_path);

        Self {
            id,
            socket_path,
            task_count: AtomicUsize::new(0),
            python_worker,
        }
    }

    pub async fn call(
        &self,
        scope: Scope<impl State + Serialize>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.task_count.fetch_add(1, Ordering::SeqCst);

        let mut stream = UnixStream::connect(&self.socket_path).await?;
        let scope_message = rmp_serde::to_vec_named(&scope)?;
        self.send_message_to_py(&mut stream, scope_message).await?;

        let mut should_continue;
        loop {
            should_continue = tokio::select! {
                msg = self.receive_message_from_py(&mut stream) => {
                    let msg = msg?;
                    if &msg == b"done" {
                        false
                    } else {
                        let msg: ASGISendEvent = rmp_serde::from_slice(&msg)?;
                        send(msg).await?;
                        true
                    }
                },
                msg = receive() => {
                    let message = rmp_serde::to_vec_named(&msg)?;
                    self.send_message_to_py(&mut stream, message).await?;
                    true
                }
            };

            if !should_continue {
                break;
            }
        }

        self.task_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    async fn receive_message_from_py(&self, stream: &mut UnixStream) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut byte = [0; 1];
        let worker_id = stream.read_u64().await? as usize;
        stream.read_exact(&mut byte).await?;

        if worker_id != self.id {
            panic!("Worker {} received message meant for worker {}", self.id, worker_id);
        }

        let length = stream.read_u64().await? as usize;
        stream.read_exact(&mut byte).await?;

        let mut buf = Vec::with_capacity(length);
        stream.read_exact(&mut buf).await?;

        Ok(buf)
    }

    async fn send_message_to_py(
        &self,
        stream: &mut UnixStream,
        message: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        stream
            .write_all(format!("{}|{}|", self.id, message.len()).as_bytes())
            .await?;
        stream.write_all(&message).await?;
        stream.write_all(b"\n").await?;
        Ok(())
    }
}
