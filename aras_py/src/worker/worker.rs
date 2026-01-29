use std::path::Path;
use std::result::Result;
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
use std::time::Duration;

use thiserror::Error;
use asgispec::prelude::*;
use asgispec::scope::LifespanScope;
use log::info;
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
    net::UnixStream,
    process::Command,
    sync::oneshot::{self, Sender},
    sync::Mutex,
};

use super::get_socket_path;

#[derive(Error, Debug, Clone)]
enum TaskError {
    #[error(transparent)]
    DisconnectedClientError(#[from] Arc<DisconnectedClient>),
    #[error(transparent)]
    IOError(#[from] Arc<std::io::Error>),
    #[error(transparent)]
    EncodeError(#[from] Arc<rmp_serde::encode::Error>),
}

type TaskResult = Result<(), TaskError>;

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
    pub fn spawn(id: usize, worker_script: &str, python: &str, import_str: &str, pythonpath: &str) -> Self {
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
    pub async fn startup(
        &self,
        scope: LifespanScope<impl State + Serialize>,
    ) -> Result<ASGISendEvent, Box<dyn std::error::Error>> {
        self.task_count.fetch_add(1, Ordering::SeqCst);
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (mut read, mut write) = stream.into_split();

        send_message_to_py(&mut write, Scope::from(scope)).await.unwrap();
        send_message_to_py(&mut write, ASGIReceiveEvent::new_lifespan_startup()).await.unwrap();
        let startup_result = receive_message_from_py(&mut read).await.unwrap();

        *self.lifespan_stream.lock().await = Some(write.reunite(read).unwrap());

        Ok(startup_result)
    }

    // Run lifespan shutdown and return the response from the worker.
    pub async fn shutdown(&self) -> Result<ASGISendEvent, Box<dyn std::error::Error>> {
        let stream = self
            .lifespan_stream
            .lock()
            .await
            .take()
            .expect("Shutdown called before startup");
        let (mut read, mut write) = stream.into_split();
        send_message_to_py(&mut write, ASGIReceiveEvent::new_lifespan_shutdown()).await.unwrap();
        let shutdown_result = receive_message_from_py(&mut read).await.unwrap();
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

        let stream = UnixStream::connect(&self.socket_path).await.unwrap();
        let (read, mut write) = stream.into_split();
        send_message_to_py(&mut write, scope).await.unwrap();

        let (py_sendx, py_recvx) = oneshot::channel();
        let (rust_sendx, rust_recvx) = oneshot::channel();

        run_python_worker_task(read, send, py_sendx);
        run_server_worker_task(write, receive, rust_sendx);

        let result = tokio::join!(py_recvx, rust_recvx);

        println!("Worker task completed");

        result.0??;
        result.1??;

        self.task_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

fn run_server_worker_task(mut stream: OwnedWriteHalf, receive: ReceiveFn, sender: Sender<TaskResult>) {
    tokio::task::spawn(async move {
        let result = loop {
            let message = receive().await;
            let final_msg = message.is_final();

            if let Err(e) = send_message_to_py(&mut stream, message).await {
                break e;
            }

            if final_msg {
                break Ok(())
            }
        };

        let _ = sender.send(result);
    });
}

fn run_python_worker_task(mut stream: OwnedReadHalf, send: SendFn, sender: Sender<TaskResult>) {
    tokio::task::spawn(async move {
        let result = loop {
            let message = match receive_message_from_py(&mut stream).await {
                Ok(message) => message,
                Err(e) => break e,
            };
            let final_msg = message.is_final();
            println!("{}", message.variant_str());

            if let Err(e) = send(message).await {
                break Err(TaskError::from(Arc::new(e)));
            };

            if final_msg {
                break Ok(());
            }
        };
        
        dbg!(&result);
        let _ = sender.send(result);
    });
}

async fn receive_message_from_py(stream: &mut OwnedReadHalf) -> Result<ASGISendEvent, TaskResult> {
    let mut reader = BufReader::new(stream);

    println!("Reading Python message size");
    let mut buf = [0u8; 4];
    let _ = reader.read_exact(&mut buf).await.unwrap();
    dbg!(&buf);
    let length = u32::from_be_bytes(buf) as usize;

    dbg!(length);
    let mut data_buf = vec![0u8; length];
    reader.read_exact(&mut data_buf).await.unwrap();
    Ok(rmp_serde::from_slice(&data_buf).unwrap())
}

async fn send_message_to_py(stream: &mut OwnedWriteHalf, message: impl Serialize) -> Result<(), TaskResult> {
    let data = rmp_serde::to_vec_named(&message).unwrap();
    let length = (data.len() as u32).to_be_bytes();

    stream.write_all(&length).await.unwrap();
    stream.write_all(&data).await.unwrap();
    stream.flush().await.unwrap();

    Ok(())
}

fn wait_for_socket(socket_path: &str) {
    let path = Path::new(socket_path);
    while !path.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
}

// #[derive(Debug)]
// enum WorkerState {
//     BothRunning,
//     BothStopped,
//     PythonRunning,
//     ASGIRunning,
// }

// impl WorkerState {
//     async fn transition(
//         self,
//         recv: &mut mpsc::Receiver<ASGISendEvent>,
//         stream: &mut OwnedWriteHalf,
//         send: SendFn,
//         receive: ReceiveFn,
//     ) -> Result<WorkerState, Box<dyn std::error::Error>> {
//         println!("Waiting for event in state: {:?}", self);
//         let event = match self {
//             WorkerState::BothStopped => return Ok(WorkerState::BothStopped),
//             WorkerState::PythonRunning => Event::from(recv.recv().await.unwrap()),
//             WorkerState::ASGIRunning => Event::from(receive().await),
//             WorkerState::BothRunning => {
//                 tokio::select! {
//                     // TODO: what about 2 tasks with a oneshot channel?
//                     // state machine would not be necessary
//                     msg = receive() => Event::from(msg),
//                     msg = recv.recv() => Event::from(msg.unwrap()),
//                 }
//             }
//         };

//         event.execute(stream, send).await?;
//         Ok(self.next(&event))
//     }

//     pub fn next(self, event: &Event) -> WorkerState {
//         match (self, event, event.is_final()) {
//             (this, _, false) => this,
//             (WorkerState::BothRunning, Event::ReceivedFromPython(_), true) => WorkerState::ASGIRunning,
//             (WorkerState::BothRunning, Event::ReceivedFromASGI(_), true) => WorkerState::PythonRunning,
//             (WorkerState::PythonRunning, _, true) => WorkerState::BothStopped,
//             (WorkerState::ASGIRunning, _, true) => WorkerState::BothStopped,
//             (WorkerState::BothStopped, _, _) => WorkerState::BothStopped,
//         }
//     }
// }

// enum Event {
//     ReceivedFromPython(ASGISendEvent),
//     ReceivedFromASGI(ASGIReceiveEvent),
// }

// impl Event {
//     pub async fn execute(&self, stream: &mut OwnedWriteHalf, send: SendFn) -> Result<(), Box<dyn std::error::Error>> {
//         match self {
//             Event::ReceivedFromPython(msg) => send(msg.clone()).await?,
//             Event::ReceivedFromASGI(msg) => send_message_to_py(stream, msg).await?,
//         };
//         Ok(())
//     }

//     pub fn is_final(&self) -> bool {
//         match self {
//             Event::ReceivedFromASGI(msg) => msg.is_final(),
//             Event::ReceivedFromPython(msg) => msg.is_final(),
//         }
//     }
// }

// impl From<ASGIReceiveEvent> for Event {
//     fn from(data: ASGIReceiveEvent) -> Self {
//         Self::ReceivedFromASGI(data)
//     }
// }

// impl From<ASGISendEvent> for Event {
//     fn from(data: ASGISendEvent) -> Self {
//         Self::ReceivedFromPython(data)
//     }
// }

// impl Display for Event {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             Event::ReceivedFromASGI(msg) => write!(f, "ASGI event: {}", msg),
//             Event::ReceivedFromPython(msg) => write!(f, "Python event: {}", msg),
//         }
//     }
// }
