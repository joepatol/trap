extern crate aras as aras_core;

use std::time::Duration;

use aras_core::ArasServer;
use asgispec::prelude::*;
use log::info;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes;
use tokio::runtime::Handle;

mod convert;
mod wrappers;

use tokio_util::sync::CancellationToken;
use wrappers::{PyASGIAppWrapper, PyState, PyStopServerToken};

fn get_log_level_filter(log_level: &str) -> tracing::Level {
    match log_level {
        "DEBUG" => tracing::Level::DEBUG,
        "INFO" => tracing::Level::INFO,
        "ERROR" => tracing::Level::ERROR,
        "OFF" => tracing::Level::ERROR,
        "TRACE" => tracing::Level::TRACE,
        "WARN" => tracing::Level::WARN,
        _ => tracing::Level::INFO,
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
    request_timeout = 180,
    rate_limit = (1000, 1),
    buffer_size = 1024,
    backpressure_timeout = 60,
    backpressure_size = 16,
    max_ws_frame_size = 64 * 1024,
    request_ids = false,
    auto_date_header = true,
    sensitive_headers = None,
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
/// What you probably want is to create a cancel token, run the awaitable returned by this function using `event_loop.run_until_complete`, and then when
/// you want to stop the server call `token.stop()` from another thread or signal handler.
///
/// The ARAS Python package will do this ceremony for the user when using `aras.serve` or the CLI. This lower level function is only required when the user requires
/// more control over the event loop (e.g. use something other than asyncio, or integrate into an existing event loop), or over the cancellation.
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
    request_timeout: u64,
    rate_limit: (u64, u64),
    buffer_size: usize,
    backpressure_timeout: u64,
    backpressure_size: usize,
    max_ws_frame_size: usize,
    request_ids: bool,
    auto_date_header: bool,
    sensitive_headers: Option<Vec<String>>,
) -> PyResult<Bound<'a, PyAny>> {
    tracing_subscriber::fmt()
        .with_max_level(get_log_level_filter(log_level))
        .init();

    let state = PyState::new(PyDict::new(py).unbind());
    let cancel_token = token.get_cancel_token();
    let task_locals = pyo3_async_runtimes::TaskLocals::new(event_loop).copy_context(py)?;
    let asgi_application = PyASGIAppWrapper::new(application, task_locals.clone());

    let mut builder = ArasServer::builder(cancel_token)
        .addr(addr.into())
        .port(port)
        .request_timeout(Duration::from_secs(request_timeout))
        .body_limit(max_size_kb * 1000)
        .rate_limit(rate_limit.0, rate_limit.1)
        .buffer_size(buffer_size)
        .backpressure_timeout(Duration::from_secs(backpressure_timeout))
        .backpressure_size(backpressure_size)
        .max_ws_frame_size(max_ws_frame_size);

    if request_ids {
        builder = builder.request_ids();
    }

    if keep_alive == false {
        builder = builder.no_keep_alive();
    }

    if auto_date_header == false {
        builder = builder.disable_auto_date_header();
    }

    if let Some(sensitive_headers) = sensitive_headers {
        for header in sensitive_headers.iter() {
            builder = builder.sensitive_header(
                header
                    .try_into()
                    .map_err(|e| PyValueError::new_err(format!("Invalid header name '{}'; {}", header, e)))?,
            );
        }
    }

    if let Some(limit) = max_concurrency {
        builder = builder.concurrency_limit(limit);
    }

    let asgi_server = builder.build();

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
