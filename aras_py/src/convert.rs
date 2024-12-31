use log::error;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyBytes, PyDict, PyList, PyMapping, PyNone, PyString};
use pyo3::{prelude::*, IntoPyObjectExt};

use aras_core::*;
use super::PyState;

pub fn parse_py_http_response_start(py_map: &Bound<PyMapping>) -> PyResult<ASGISendEvent> {
    let status: u16 = py_map.get_item("status")?.extract()?;
    let headers = py_map
        .get_item("headers")
        .and_then(|v| v.extract::<Vec<(Vec<u8>, Vec<u8>)>>())
        .unwrap_or(Vec::new());
    Ok(ASGISendEvent::new_http_response_start(status, headers))
}

pub fn parse_py_http_response_body(py_map: &Bound<PyMapping>) -> PyResult<ASGISendEvent> {
    let body: Vec<u8> = py_map.get_item("body")?.extract()?;
    let more_body = py_map
        .get_item("more_body")
        .and_then(|v| v.extract::<bool>())
        .unwrap_or(false);
    Ok(ASGISendEvent::new_http_response_body(body, more_body))
}

pub fn parse_startup_failed(py_map: &Bound<PyMapping>) -> ASGISendEvent {
    let message = py_map
        .get_item("message")
        .and_then(|v| v.extract())
        .unwrap_or(String::from(""));
    ASGISendEvent::new_startup_failed(message)
}

pub fn parse_shutdown_failed(py_map: &Bound<PyMapping>) -> ASGISendEvent {
    let message = py_map
        .get_item("message")
        .and_then(|v| v.extract())
        .unwrap_or(String::from(""));
    ASGISendEvent::new_shutdown_failed(message)
}

pub fn http_request_event_into_py<'py>(py: Python<'py>, event: HTTPRequestEvent) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    python_result_dict.set_item("body", PyBytes::new(py, event.body.as_slice()))?;
    python_result_dict.set_item("more_body", event.more_body.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn http_disconnect_event_into_py<'py>(py: Python<'py>, event: HTTPDisconnectEvent) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

fn asgi_scope_into_py<'py>(py: Python<'py>, scope: ASGIScope) -> PyResult<Bound<'py, PyDict>> {
    let asgi_dict = PyDict::new(py);
    asgi_dict.set_item("version", scope.version.into_pyobject(py)?)?;
    asgi_dict.set_item("spec_version", String::from(scope.spec_version).into_pyobject(py)?)?;
    Ok(asgi_dict)
}

pub fn http_scope_into_py<'py>(py: Python<'py>, scope: HTTPScope<PyState>) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", scope.type_.into_pyobject(py)?)?;
    python_result_dict.set_item("asgi", asgi_scope_into_py(py, scope.asgi)?)?;
    python_result_dict.set_item("http_version", String::from(scope.http_version).into_pyobject(py)?)?;
    python_result_dict.set_item("method", scope.method.into_pyobject(py)?)?;
    python_result_dict.set_item("scheme", scope.scheme.into_pyobject(py)?)?;
    python_result_dict.set_item("path", scope.path.into_pyobject(py)?)?;
    python_result_dict.set_item("raw_path", PyBytes::new(py, &scope.raw_path))?;
    python_result_dict.set_item("query_string", PyBytes::new(py, &scope.query_string))?;
    python_result_dict.set_item("root_path", scope.root_path.into_pyobject(py)?)?;
    let py_bytes_headers: Vec<(Bound<PyBytes>, Bound<PyBytes>)> = scope
        .headers
        .into_iter()
        .map(|(k, v)| (PyBytes::new(py, k.as_slice()), PyBytes::new(py, v.as_slice())))
        .collect();
    python_result_dict.set_item("headers", py_bytes_headers.into_pyobject(py)?)?;
    let py_client = match scope.client {
        Some(s) => PyList::new(py, vec![s.0.into_py_any(py)?, s.1.into_py_any(py)?])?.into_py_any(py),
        None => PyNone::get(py).into_py_any(py),
    };
    python_result_dict.set_item("client", py_client?)?;
    let py_server = match scope.server {
        Some(s) => PyList::new(py, vec![s.0.into_py_any(py)?, s.1.into_py_any(py)?])?.into_py_any(py),
        None => PyNone::get(py).into_py_any(py),
    };
    python_result_dict.set_item("server", py_server?)?;
    python_result_dict.set_item("state", scope.state.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn lifespan_scope_into_py<'py>(py: Python<'py>, scope: LifespanScope<PyState>) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", scope.type_.into_pyobject(py)?)?;
    python_result_dict.set_item("asgi", asgi_scope_into_py(py, scope.asgi)?)?;
    python_result_dict.set_item("state", scope.state.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn lifespan_startup_into_py<'py>(py: Python<'py>, event: LifespanStartup) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn lifespan_shutdown_into_py<'py>(py: Python<'py>, event: LifespanShutdown) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn parse_websocket_accept(py_map: &Bound<PyMapping>) -> PyResult<ASGISendEvent> {
    let subprotocol = py_map
        .get_item("subprotocol")
        .and_then(|inner| inner.extract::<String>())
        .ok();
    let headers = py_map
        .get_item("headers")
        .and_then(|v| v.extract::<Vec<(Vec<u8>, Vec<u8>)>>())
        .unwrap_or(Vec::new());
    Ok(ASGISendEvent::new_websocket_accept(subprotocol, headers))
}

pub fn parse_websocket_send(py_map: &Bound<PyMapping>) -> PyResult<ASGISendEvent> {
    let bytes = py_map
        .get_item("bytes")
        .and_then(|inner| inner.extract::<Vec<u8>>())
        .ok();
    let text = py_map
        .get_item("text")
        .and_then(|inner| inner.extract::<String>())
        .ok();

    if bytes == None && text == None {
        error!("Websocket send doesn't have a valid bytes or text field");
        return Err(PyErr::new::<PyRuntimeError, _>("Websocket send doesn't have a valid bytes or text field"))
    };

    Ok(ASGISendEvent::new_websocket_send(bytes, text))
}

pub fn parse_websocket_close(py_map: &Bound<PyMapping>) -> PyResult<ASGISendEvent> {
    let code = py_map
        .get_item("code")
        .and_then(|inner| inner.extract::<usize>())
        .unwrap_or(1000);
    let reason = py_map
        .get_item("reason")
        .and_then(|inner| inner.extract::<String>())
        .unwrap_or(String::new());

    Ok(ASGISendEvent::new_websocket_close(Some(code), reason))
}

pub fn websocket_receive_into_py<'py>(py: Python<'py>, event: WebsocketReceiveEvent) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    match event.bytes {
        Some(v) => python_result_dict.set_item("bytes", PyBytes::new(py, &v)),
        None => python_result_dict.set_item("bytes", PyNone::get(py)),
    }?;
    match event.text {
        Some(s) => python_result_dict.set_item("text", PyString::new(py, &s)),
        None => python_result_dict.set_item("text", PyNone::get(py)),
    }?;
    Ok(python_result_dict)
}

pub fn websocket_disconnect_into_py<'py>(py: Python<'py>, event: WebsocketDisconnectEvent) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    python_result_dict.set_item("code", event.code.into_pyobject(py)?)?;
    python_result_dict.set_item("reason", String::new().into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn websocket_scope_into_py<'py>(py: Python<'py>, scope: WebsocketScope<PyState>) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", scope.type_.into_pyobject(py)?)?;
    python_result_dict.set_item("asgi", asgi_scope_into_py(py, scope.asgi)?)?;
    python_result_dict.set_item("http_version", String::from(scope.http_version).into_pyobject(py)?)?;
    python_result_dict.set_item("scheme", scope.scheme.into_pyobject(py)?)?;
    python_result_dict.set_item("path", scope.path.into_pyobject(py)?)?;
    python_result_dict.set_item("raw_path", PyBytes::new(py, &scope.raw_path))?;
    python_result_dict.set_item("query_string", PyBytes::new(py, &scope.query_string))?;
    python_result_dict.set_item("root_path", scope.root_path.into_pyobject(py)?)?;
    let py_bytes_headers: Vec<(Bound<PyBytes>, Bound<PyBytes>)> = scope
        .headers
        .into_iter()
        .map(|(k, v)| (PyBytes::new(py, k.as_slice()), PyBytes::new(py, v.as_slice())))
        .collect();
    python_result_dict.set_item("headers", py_bytes_headers.into_pyobject(py)?)?;
    let py_client = match scope.client {
        Some(s) => PyList::new(py, vec![s.0.into_py_any(py)?, s.1.into_py_any(py)?])?.into_py_any(py),
        None => PyNone::get(py).into_py_any(py),
    };
    python_result_dict.set_item("client", py_client?)?;
    let py_server = match scope.server {
        Some(s) => PyList::new(py, vec![s.0.into_py_any(py)?, s.1.into_py_any(py)?])?.into_py_any(py),
        None => PyNone::get(py).into_py_any(py),
    };
    python_result_dict.set_item("server", py_server?)?;
    let py_subprotocols: Vec<Bound<PyString>> = scope
        .subprotocols
        .into_iter()
        .map(|subprotocol| PyString::new(py, &subprotocol))
        .collect();
    python_result_dict.set_item("subprotocols", py_subprotocols.into_pyobject(py)?)?;
    python_result_dict.set_item("state", scope.state.into_pyobject(py)?)?;
    Ok(python_result_dict)
}

pub fn websocket_connect_into_py<'py>(py: Python<'py>, event: WebsocketConnectEvent) -> PyResult<Bound<'py, PyDict>> {
    let python_result_dict = PyDict::new(py);
    python_result_dict.set_item("type", event.type_.into_pyobject(py)?)?;
    Ok(python_result_dict)
}
