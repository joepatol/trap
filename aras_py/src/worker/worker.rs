use std::path::Path;
use std::process::{Child, Command};
use std::result::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use log::info;
use asgispec::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::{
    io::{AsyncWriteExt, BufReader, AsyncBufReadExt},
    net::UnixStream,
};

use super::SOCKET_PATH;

fn get_socket_path(worker_id: usize) -> String {
    format!("{SOCKET_PATH}worker-{worker_id}.sock")
}

fn wait_for_socket(socket_path: &str) {
    let path = Path::new(socket_path);
    while !path.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn start_python_worker(
    id: usize,
    python: &str,
    worker_script: &str,
    socket_path: &str,
    pythonpath: &str,
    import_str: &str
) -> Child {
    Command::new(python)
        .arg(worker_script)
        .args(["--socket", socket_path, "--id", &id.to_string(), "--app", import_str])
        .env("PYTHONPATH", pythonpath)
        .spawn()
        .expect(&format!("Failed to start Python worker {}", id))
}

enum Event {
    ReceivedFromPython(Vec<u8>),
    ReceivedFromASGI(ASGIReceiveEvent),
}

impl From<ASGIReceiveEvent> for Event {
    fn from(data: ASGIReceiveEvent) -> Self {
        Event::ReceivedFromASGI(data)
    }
}

impl From<Vec<u8>> for Event {
    fn from(data: Vec<u8>) -> Self {
        Event::ReceivedFromPython(data)
    }
}

pub(crate) fn spawn_worker(
    id: usize,
    worker_script: &str,
    python: &str,
    import_str: &str,
    pythonpath: &str,
) -> Worker {
        info!("Starting worker {id}");
        let socket_path = get_socket_path(id);
        let _ = std::fs::remove_file(&socket_path);
        let _python_worker = start_python_worker(id, python, worker_script, &socket_path, pythonpath, import_str);
        wait_for_socket(&socket_path);

        let worker = Worker::new(id, socket_path, _python_worker);
        info!("Worker {id} started");
        worker
}

#[derive(Serialize, Deserialize)]
struct InfoMessage {
    worker_id: usize,
    length: usize,
}

pub(crate) struct Worker {
    pub task_count: AtomicUsize,
    pub id: usize,
    socket_path: String,
    _python_worker: Child,
}

impl Worker {
    pub fn new(id: usize, socket_path: String, python_worker: Child) -> Self {
        Self {
            task_count: AtomicUsize::new(0),
            id,
            socket_path,
            _python_worker: python_worker,
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

        loop {
            let event = tokio::select! {
                msg = receive() => Event::from(msg),
                msg = self.receive_message_from_py(&mut stream) => Event::from(msg?)
            };

            match event {
                Event::ReceivedFromASGI(asgi_event) => {
                    let message = rmp_serde::to_vec_named(&asgi_event)?;
                    self.send_message_to_py(&mut stream, message).await?;
                }
                Event::ReceivedFromPython(data) => {
                    if &data == b"done" {
                        break;
                    }

                    let parsed_msg: ASGISendEvent = rmp_serde::from_slice(&data)?;
                    send(parsed_msg).await?;
                }
            }
        }

        self.task_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    async fn receive_message_from_py(&self, stream: &mut UnixStream) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut reader = BufReader::new(stream);

        let mut info_buffer = Vec::new();
        let size = reader.read_until(b'|', &mut info_buffer).await?;
        let message: InfoMessage = rmp_serde::from_slice(&info_buffer[..size - 1])?;

        if message.worker_id != self.id {
            panic!("Worker {} received message meant for worker {}", self.id, message.worker_id);
        }

        let mut data_buffer = vec![0u8; message.length];
        reader.read_exact(&mut data_buffer).await?;

        Ok(data_buffer)
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