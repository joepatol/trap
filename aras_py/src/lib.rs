extern crate aras as aras_core;

use tokio::runtime::Handle;
use aras_core::ServerConfig;
use log::{debug, error, info};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes;
use simplelog::*;

mod convert;
mod wrappers;

use wrappers::{PyASGIAppWrapper, PyState};

fn terminate_python_event_loop(py: Python, event_loop: Py<PyAny>) -> PyResult<()> {
    let event_loop_stop_fn = event_loop.getattr(py, "stop")?;
    event_loop.call_method1(py, "call_soon_threadsafe", (event_loop_stop_fn,))?;
    Ok(())
}

fn run_python_event_loop(event_loop: Bound<PyAny>) -> Result<(), ()> {
    let running_loop = (event_loop).call_method0("run_forever");
    if running_loop.is_err() {
        error!("Python event loop quit unexpectedly");
        return Err(());
    };
    Ok(())
}

fn new_python_event_loop(py: Python) -> PyResult<Bound<PyAny>> {
    let module = match py.import("uvloop") {
        Ok(evl) => {
            info!("Using uvloop for Python event loop");
            evl.call_method0("install")?;
            Ok(evl)
        }
        Err(_) => {
            info!("Uvloop not installed, using asyncio for Python event loop");
            py.import("asyncio")
        }
    }?;

    module.call_method0("new_event_loop")
}

fn get_log_level_filter(log_level: &str) -> LevelFilter {
    match log_level {
        "DEBUG" => LevelFilter::Debug,
        "INFO" => LevelFilter::Info,
        "ERROR" => LevelFilter::Error,
        "OFF" => LevelFilter::Off,
        "TRACE" => LevelFilter::Trace,
        "WARN" => LevelFilter::Warn,
        _ => LevelFilter::Info,
    }
}

// Serve the ASGI application
#[pyfunction]
#[pyo3(signature = (
    application, 
    addr = [127, 0, 0, 1], 
    port = 8080, 
    keep_alive = true, 
    log_level = "INFO", 
    max_concurrency = None,
    max_size_kb = 1_000_000,
))]
fn serve(
    py: Python,
    application: Py<PyAny>,
    addr: [u8; 4],
    port: u16,
    keep_alive: bool,
    log_level: &str,
    max_concurrency: Option<usize>,
    max_size_kb: u64,
) -> PyResult<()> {
    SimpleLogger::init(get_log_level_filter(log_level), Config::default())
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to start logger. {}", e)))?;
    let config = ServerConfig::new(keep_alive, max_concurrency, addr.into(), port, max_size_kb * 1000);
    let state = PyState::new(PyDict::new(py).unbind()); // State dictionary for the ASGI application

    // asyncio setup
    let asyncio = py.import("asyncio")?;
    let event_loop = new_python_event_loop(py)?;
    let event_loop_clone = event_loop.clone().into();
    asyncio.call_method1("set_event_loop", (&event_loop,))?;

    // TaskLocals stores a reference to the event loop, which can be used to run Python coroutines
    let task_locals = pyo3_async_runtimes::TaskLocals::new(event_loop.clone().into()).copy_context(py)?;

    // Run Rust event loop with the server in a separate thread
    let server_task = std::thread::spawn(move || {
        let server_result = Python::with_gil(|py| {
            pyo3_async_runtimes::tokio::run(py, async move {
                info!("Started {} workers", Handle::current().metrics().num_workers());

                let asgi_application = PyASGIAppWrapper::new(application, task_locals);

                ::aras::serve(asgi_application, state, Some(config))
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("Error running server; {}", e.to_string())))
            })
        });

        // When the server is done, stop Python's event loop as well
        debug!("Terminate Python event loop");
        if let Err(e) = Python::with_gil(|py| terminate_python_event_loop(py, event_loop_clone)) {
            return Err(e);
        };

        server_result
    });

    // Python's event loop runs in the main thread
    if let Err(_) = run_python_event_loop(event_loop) {
        return Err(PyRuntimeError::new_err(
            "Python event loop quit, cannot shutdown gracefully",
        ));
    }

    server_task
        .join()
        .map_err(|e| PyRuntimeError::new_err(format!("{e:?}")))??;
    Ok(())
}

#[pymodule]
fn aras(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(serve, m)?)?;
    Ok(())
}
