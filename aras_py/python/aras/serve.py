import asyncio
import importlib
import os
import signal
import sys
from typing import Any, overload

from .aras import generate_cancel_token, serve_python  # type: ignore
from .types import ASGIApplication, LogLevel


@overload
def serve(
    application: ASGIApplication,
    host: str = ...,
    port: int = ...,
    log_level: LogLevel = ...,
    keep_alive: bool = ...,
    max_concurrency: int | None = ...,
    max_size_kb: int = ...,
    request_timeout: int = ...,
    rate_limit: tuple[int, int] = ...,
    buffer_size: int = ...,
    backpressure_timeout: int = ...,
    backpressure_size: int = ...,
    max_ws_frame_size: int = ...,
    request_ids: bool = ...,
    auto_date_header: bool = ...,
    sensitive_headers: list[str] | None = ...,
    reload: bool = ...,
) -> None: ...


@overload
def serve(
    application: str,
    host: str = ...,
    port: int = ...,
    log_level: LogLevel = ...,
    keep_alive: bool = ...,
    max_concurrency: int | None = ...,
    max_size_kb: int = ...,
    request_timeout: int = ...,
    rate_limit: tuple[int, int] = ...,
    buffer_size: int = ...,
    backpressure_timeout: int = ...,
    backpressure_size: int = ...,
    max_ws_frame_size: int = ...,
    request_ids: bool = ...,
    auto_date_header: bool = ...,
    sensitive_headers: list[str] | None = ...,
    reload: bool = ...,
) -> None: ...


def serve(
    application: ASGIApplication | str,
    host: str = "127.0.0.1",
    port: int = 8080,
    log_level: LogLevel = "INFO",
    keep_alive: bool = True,
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
    request_timeout: int = 180,
    rate_limit: tuple[int, int] = (1000, 1),
    buffer_size: int = 1024,
    backpressure_timeout: int = 60,
    backpressure_size: int = 16,
    max_ws_frame_size: int = 64 * 1024,
    request_ids: bool = False,
    auto_date_header: bool = True,
    sensitive_headers: list[str] | None = None,
    reload: bool = False,
) -> None:
    kwargs = dict(
        host=host,
        port=port,
        log_level=log_level,
        keep_alive=keep_alive,
        max_concurrency=max_concurrency,
        max_size_kb=max_size_kb,
        request_timeout=request_timeout,
        rate_limit=rate_limit,
        buffer_size=buffer_size,
        backpressure_timeout=backpressure_timeout,
        backpressure_size=backpressure_size,
        max_ws_frame_size=max_ws_frame_size,
        request_ids=request_ids,
        auto_date_header=auto_date_header,
        sensitive_headers=sensitive_headers,
    )

    if reload:
        import_string = _resolve_import_string(application)
        _serve_with_reload(import_string, **kwargs)
    else:
        app = _import_from_string(application) if isinstance(application, str) else application
        _serve(app, **kwargs)  # type: ignore


def _resolve_import_string(application: ASGIApplication | str) -> str:
    if isinstance(application, str):
        return application

    module_name = getattr(application, "__module__", None)
    if module_name is None:
        raise ValueError(
            "Cannot determine the import string for hot reload: the application has no '__module__' attribute. "
            "Pass an import string like 'myapp.main:app' instead."
        )

    module = sys.modules.get(module_name)
    if module is None:
        raise ValueError(
            f"Cannot determine the import string for hot reload: module '{module_name}' is not in sys.modules. "
            "Pass an import string like 'myapp.main:app' instead."
        )

    attr_name = next((k for k, v in vars(module).items() if v is application), None)
    if attr_name is None:
        raise ValueError(
            f"Cannot determine the import string for hot reload: the application was not found as a "
            f"module-level attribute in '{module_name}'. "
            "Pass an import string like 'myapp.main:app' instead."
        )

    return f"{module_name}:{attr_name}"


def _import_from_string(import_string: str) -> ASGIApplication:
    module_str, attr_str = import_string.split(":")
    module = importlib.import_module(module_str)
    return getattr(module, attr_str)


def _serve_with_reload(import_string: str, **kwargs) -> None:  # type: ignore[no-untyped-def]
    try:
        from watchfiles import DefaultFilter, run_process
    except ImportError:
        raise ImportError(
            "watchfiles is required for hot reload. Please install it with 'pip install aras_py[hot-reload]'."
        )

    run_process(
        ".",
        target=_serve_from_import_string,
        args=(import_string,),
        callback=_files_changed_callback,
        kwargs=kwargs,
        watch_filter=DefaultFilter(),
    )


def _files_changed_callback(file_changes: set[tuple[Any, str]]) -> None:
    changed_files = {f for _, f in file_changes}
    print(f"Files changed: {':'.join(changed_files)}.\nRestarting server...")


def _serve_from_import_string(import_string: str, **kwargs) -> None:  # type: ignore[no-untyped-def]
    sys.path.insert(0, os.getcwd())
    app = _import_from_string(import_string)
    _serve(app, **kwargs)


def _serve(
    application: ASGIApplication,
    host: str = "127.0.0.1",
    port: int = 8080,
    log_level: LogLevel = "INFO",
    keep_alive: bool = True,
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
    request_timeout: int = 180,
    rate_limit: tuple[int, int] = (1000, 1),
    buffer_size: int = 1024,
    backpressure_timeout: int = 60,
    backpressure_size: int = 16,
    max_ws_frame_size: int = 64 * 1024,
    request_ids: bool = False,
    auto_date_header: bool = True,
    sensitive_headers: list[str] | None = None,
) -> None:
    loop = asyncio.new_event_loop()
    token = generate_cancel_token()

    loop.add_signal_handler(signal.SIGINT, token.stop)
    loop.add_signal_handler(signal.SIGTERM, token.stop)

    loop.run_until_complete(
        serve_python(
            application,
            token,
            loop,
            addr=[int(i) for i in host.split(".")],
            port=port,
            keep_alive=keep_alive,
            log_level=log_level,
            max_concurrency=max_concurrency,
            max_size_kb=max_size_kb,
            request_timeout=request_timeout,
            rate_limit=rate_limit,
            buffer_size=buffer_size,
            backpressure_timeout=backpressure_timeout,
            backpressure_size=backpressure_size,
            max_ws_frame_size=max_ws_frame_size,
            request_ids=request_ids,
            auto_date_header=auto_date_header,
            sensitive_headers=sensitive_headers,
        )
    )
