use std::fmt::Display;
use std::path::Path;
use std::result::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use asgispec::prelude::*;
use asgispec::scope::LifespanScope;
use log::info;
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::Command,
    sync::oneshot::{self, Sender},
    sync::Mutex,
};

use super::get_socket_path;

/// Represent a single worker.
/// 
/// A worker is running a Python process that listens on a unix socket.
/// This worker serves as an adapter between the ASGI server and the Python ASGI
/// application running in the Python process. It translates ASGI events to and from
/// socket messages.
pub(crate) struct Worker {
    pub id: usize,
    pub task_count: AtomicUsize,
    socket_path: String,
    py_handle: Mutex<Option<Sender<()>>>,
    lifespan_stream: Mutex<Option<UnixStream>>,
}

impl Worker {
    // Spawn a new worker process.
    // Starts a Python process running a Python unix socket server that
    // runs the ASGI application.
    pub fn spawn(
        id: usize,
        worker_script: &str,
        python: &str,
        import_str: &str,
        pythonpath: &str,
    ) -> Self {
        info!("Spawning worker {id}");
        let socket_path = get_socket_path(id);
        let _ = std::fs::remove_file(&socket_path);

        let (rx, tx) = oneshot::channel::<()>();

        let mut python_worker = Command::new(python)
            .arg(worker_script)
            .args(["--socket", &socket_path, "--app", &import_str])
            .env("PYTHONPATH", &pythonpath)
            .spawn()
            .expect("Failed to start Python worker");

        tokio::task::spawn(async move {
            tokio::select! {
                _ = tx => python_worker.kill().await.map(|_| true).expect(&format!("Failed to kill worker {} process", id)),
                status = python_worker.wait() => status.map(|_| false).expect(&format!("Error while waiting on worker {} process", id)),
            };
        });

        wait_for_socket(&socket_path);
        let worker = Self {
            id,
            socket_path,
            py_handle: Mutex::new(Some(rx)),
            task_count: AtomicUsize::new(0),
            lifespan_stream: Mutex::new(None),
        };

        info!("Worker {id} spawned");
        worker
    }

    // Stop the worker process.
    pub async fn stop(&self) {
        println!("Stopping worker {}", self.id);
        let _ = self.py_handle.lock().await.take().unwrap().send(());
        println!("Worker {} stopped", self.id);
    }

    // Run lifespan startup and return the response from the worker.
    pub async fn startup(&self, scope: LifespanScope<impl State + Serialize>) -> Result<ASGISendEvent, Box<dyn std::error::Error>> {
        self.task_count.fetch_add(1, Ordering::SeqCst);
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        send_message_to_py(&mut stream, Scope::from(scope)).await?;
        send_message_to_py(&mut stream, ASGIReceiveEvent::new_lifespan_startup()).await?;
        let startup_result = receive_message_from_py(&mut stream).await?;
        
        *self.lifespan_stream.lock().await = Some(stream);

        Ok(startup_result)
    }

    // Run lifespan shutdown and return the response from the worker.
    pub async fn shutdown(&self) -> Result<ASGISendEvent, Box<dyn std::error::Error>> {
        let mut stream = self.lifespan_stream.lock().await.take().expect("Shutdown called before startup");
        send_message_to_py(&mut stream, ASGIReceiveEvent::new_lifespan_shutdown()).await?;
        let shutdown_result = receive_message_from_py(&mut stream).await?;
        self.task_count.fetch_sub(1, Ordering::SeqCst);
        Ok(shutdown_result)
    }

    // Call the worker to handle a connection using the ASGI protocol.
    pub async fn call(
        &self,
        scope: Scope<impl State + Serialize>,
        receive: ReceiveFn,
        send: SendFn,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.task_count.fetch_add(1, Ordering::SeqCst);

        let mut stream = UnixStream::connect(&self.socket_path).await?;
        send_message_to_py(&mut stream, scope).await?;

        let mut asgi_disconnected = false;
        let mut py_disconnected = false;

        loop {
            if asgi_disconnected && py_disconnected {
                break;
            };

            let event = if asgi_disconnected {
                let msg = receive_message_from_py(&mut stream).await?;
                Event::from(msg)
            } else if py_disconnected {
                let msg = receive().await;
                Event::from(msg)
            } else {
                tokio::select! {
                    msg = receive() => Event::from(msg),
                    msg = receive_message_from_py(&mut stream) => Event::from(msg?)
                }
            };

            println!("Worker {} received {}", self.id, event);

            match event {
                Event::ReceivedFromASGI(message) => {
                    asgi_disconnected = message.is_final();
                    send_message_to_py(&mut stream, message).await?
                }
                Event::ReceivedFromPython(message) => {
                    py_disconnected = message.is_final();
                    println!("py disconnected: {}", py_disconnected);
                    send(message).await?;
                }
            }
        }

        self.task_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
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

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::ReceivedFromASGI(msg) => write!(f, "ASGI event: {}", msg),
            Event::ReceivedFromPython(msg) => write!(f, "Python event: {}", msg),
        }
    }
}

async fn receive_message_from_py(stream: &mut UnixStream) -> Result<ASGISendEvent, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream);

    let mut buf = [0u8; 4];
    let _ = reader.read_exact(&mut buf).await?;
    let length = u32::from_be_bytes(buf) as usize;

    let mut data_buf = vec![0u8; length];
    reader.read_exact(&mut data_buf).await?;

    Ok(rmp_serde::from_slice(&data_buf)?)
}

async fn send_message_to_py(
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

fn wait_for_socket(socket_path: &str) {
    let path = Path::new(socket_path);
    while !path.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
}
