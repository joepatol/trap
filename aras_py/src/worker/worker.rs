use std::sync::Arc;
use std::path::Path;
use std::process::{Child, Command};
use std::result::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use log::info;
use asgispec::prelude::*;
use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::UnixStream,
};

use super::get_socket_path;

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

        let _python_worker = Command::new(python)
            .arg(worker_script)
            .args(["--socket", &socket_path, "--id", &id.to_string(), "--app", import_str])
            .env("PYTHONPATH", pythonpath)
            .spawn()
            .expect(&format!("Failed to start Python worker {}", id));
        
        wait_for_socket(&socket_path);
        let worker = Worker::new(id, socket_path, _python_worker);
        
        info!("Worker {id} started");
        worker
}

fn wait_for_socket(socket_path: &str) {
    let path = Path::new(socket_path);
    while !path.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
}

enum Event {
    ReceivedFromPython(ASGISendEvent),
    ReceivedFromASGI(ASGIReceiveEvent),
}

impl From<ASGIReceiveEvent> for Event {
    fn from(data: ASGIReceiveEvent) -> Self {
        Self::ReceivedFromASGI(data)
    }
}

impl From<ASGISendEvent> for Event {
    fn from(data: ASGISendEvent) -> Self {
        Self::ReceivedFromPython(data)
    }
}

pub(crate) struct Worker {
    pub task_count: AtomicUsize,
    pub id: usize,
    socket_path: String,
    python_worker: Child,
}

impl Worker {
    pub fn new(id: usize, socket_path: String, python_worker: Child) -> Self {
        Self {
            task_count: AtomicUsize::new(0),
            id,
            socket_path,
            python_worker,
        }
    }

    // pub fn stop(&mut self) {
    //     println!("Stopping worker {}", self.id);
    //     let _ = self.python_worker.kill().expect(&format!("Failed to stop worker {}", self.id));
    //     println!("Worker {} stopped", self.id);
    // }

    // pub async fn watch(&self, func: impl FnOnce(Child)) {
    //     let child = Arc::clone(&self.python_worker);
    //     tokio::spawn(async move {
    //         let status = child.wait().expect("Failed to wait on child process");
    //         println!("Worker exited with status: {}", status);
    //         func(child);
    //     });
    // }

    pub async fn call(
        &self,
        scope: Scope<impl State + Serialize>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.task_count.fetch_add(1, Ordering::SeqCst);

        let mut stream = UnixStream::connect(&self.socket_path).await?;
        self.send_message_to_py(&mut stream, scope).await?;

        let mut asgi_disconnected = false;
        let mut py_disconnected = false;

        loop {
            if asgi_disconnected && py_disconnected {
                break;
            };

            let event = if asgi_disconnected {
                let msg = self.receive_message_from_py(&mut stream).await?;
                Event::from(msg)
            } else if py_disconnected {
                let msg = receive().await;
                Event::from(msg)
            } else {
                tokio::select! {
                    msg = receive() => Event::from(msg),
                    msg = self.receive_message_from_py(&mut stream) => Event::from(msg?)
                }
            };

            match event {
                Event::ReceivedFromASGI(message) => {
                    asgi_disconnected = message.is_final();
                    self.send_message_to_py(&mut stream, message).await?
                }
                Event::ReceivedFromPython(message) => {
                    py_disconnected = message.is_final();
                    send(message).await?;
                }
            }
        }

        self.task_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    async fn receive_message_from_py(&self, stream: &mut UnixStream) -> Result<ASGISendEvent, Box<dyn std::error::Error>> {
        let mut reader = BufReader::new(stream);

        let mut buf = [0u8; 4];
        let _ = reader.read_exact(&mut buf).await?;
        let length = u32::from_be_bytes(buf) as usize;

        let mut data_buf = vec![0u8; length];
        reader.read_exact(&mut data_buf).await?;

        Ok(rmp_serde::from_slice(&data_buf)?)
    }

    async fn send_message_to_py(
        &self,
        stream: &mut UnixStream,
        message: impl Serialize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = rmp_serde::to_vec_named(&message)?;
        let length = (data.len() as u32).to_be_bytes();

        stream.write_all(&length).await?;
        stream.write_all(&data).await?;
        stream.flush().await?;

        Ok(())
    }
}