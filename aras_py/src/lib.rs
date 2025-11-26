extern crate aras as aras_core;

use std::time::Duration;

use aras_core::ArasServer;
use asgispec::prelude::*;
use log::info;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes;
use simplelog::*;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;

mod convert;
mod wrappers;

use tokio_util::sync::CancellationToken;
use wrappers::{PyASGIAppWrapper, PyState, PyStopServerToken};

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

#[pyfunction]
#[pyo3(signature = ())]
/// Get a new cancel token for stopping the server from Python.
/// Exclusively useful for `aras.serve_python`
fn generate_cancel_token() -> PyStopServerToken {
    let token = CancellationToken::new();
    PyStopServerToken::new(token)
}

#[pyfunction]
#[pyo3(signature = (
    application,
    token,
    event_loop,
    *,
    addr = [127, 0, 0, 1],
    port = 8080,
    log_level = "INFO",
    keep_alive = true,
    max_concurrency = None,
    max_size_kb = 1000000,
))]
/// Serves a Python ASGI application using the ARAS server.
///
/// Configuration of the server is done through the provided parameters.
///
/// This function requires the Python event loop to be started on the Python side and passed as an argument, a Python awaitable will be returned that resolves
/// when the server process ends. This way the control and managing of the Python event loop is left completely on the Python side.
///
/// A cancellation token is also required to allow shutdown of the server from Python, the token can be generated using the `generate_cancel_token` function.
///
/// What you probably want is to create a cancel token, run this function using `event_loop.run_until_complete`, and then when you want to stop the server call
/// token.stop() from another thread or signal handler.
///
/// The ARAS Python package will do this ceremony for the user when using `aras.serve`, hence we define `serve_python` as it's a lower level function not intended
/// to be used directly by end users.
fn serve_python<'a>(
    py: Python<'a>,
    application: Py<PyAny>,
    token: PyStopServerToken,
    event_loop: Bound<'_, PyAny>,
    addr: [u8; 4],
    port: u16,
    log_level: &str,
    keep_alive: bool,
    max_concurrency: Option<usize>,
    max_size_kb: usize,
) -> PyResult<Bound<'a, PyAny>> {
    SimpleLogger::init(get_log_level_filter(log_level), Config::default())
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to start logger. {}", e)))?;

    let state = PyState::new(PyDict::new(py).unbind());
    let cancel_token = token.get_cancel_token();
    let task_locals = pyo3_async_runtimes::TaskLocals::new(event_loop).copy_context(py)?;
    let asgi_application = PyASGIAppWrapper::new(application, task_locals.clone_ref(py));

    let asgi_server = ArasServer::new(
        addr.into(),
        port,
        keep_alive,
        Duration::from_secs(60),
        max_size_kb * 1000,
        max_concurrency.unwrap_or(Semaphore::MAX_PERMITS),
        cancel_token,
    );

    pyo3_async_runtimes::tokio::future_into_py_with_locals(py, task_locals, async move {
        info!("Started {} workers", Handle::current().metrics().num_workers());

        asgi_server
            .run(asgi_application, state)
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("Error running server; {}", e)))
    })
}

#[pymodule]
fn aras(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(serve_python, m)?)?;
    m.add_function(wrap_pyfunction!(generate_cancel_token, m)?)?;
    Ok(())
}
