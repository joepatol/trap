import asyncio
import importlib
import os
import signal
import sys
from dataclasses import dataclass, field
from pathlib import Path

from watchfiles import BaseFilter, run_process
from watchfiles.main import Change

from .aras import generate_cancel_token, serve_python  # type: ignore
from .types import ASGIApplication


@dataclass
class ServerConfig:
    host: str
    port: int
    keep_alive: bool
    max_concurrency: int | None
    max_size_kb: int
    request_timeout: int
    rate_limit: tuple[int, int]
    buffer_size: int
    backpressure_timeout: int
    backpressure_size: int
    max_ws_frame_size: int
    request_ids: bool
    auto_date_header: bool
    sensitive_headers: list[str] | None


@dataclass
class ReloadConfig:
    paths: list[str | Path] = field(default_factory=lambda: ["."])
    watch_filter: BaseFilter | None = None


def serve(
    application: ASGIApplication | str,
    host: str = "127.0.0.1",
    port: int = 8080,
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
    reload: ReloadConfig | None = None,
) -> None:
    """
    Serve an ASGI application with the given configuration.
    
    Args:
        application: The ASGI application to serve, or an import string like 'myapp.main:app'.
        host: The host to bind the server to.
        port: The port to listen on.
        log_level: The logging level to use.
        keep_alive: Whether to enable HTTP keep-alive.
        max_concurrency: The maximum number of concurrent requests to allow. None means no limit.
        max_size_kb: The maximum request size in kilobytes.
        request_timeout: The maximum time in seconds to allow for a request before timing out.
        rate_limit: A tuple of (max_requests, per_seconds) to limit the number of requests from a single client.
        buffer_size: The size in bytes of the buffer to use for reading request bodies.
        backpressure_timeout: The maximum time in seconds to wait for communication to the app before applying backpressure.
        backpressure_size: The size number of messages in a communcation channel before applying backpressure.
        max_ws_frame_size: The maximum size in bytes of a WebSocket frame. Bigger frames are fragmented.
        request_ids: Whether to generate unique request IDs for each incoming request. ID is included in the response headers.
        auto_date_header: Whether to automatically add a Date header to responses.
        sensitive_headers: A list of header names to treat as sensitive. Will be redacted in logs.
        reload: A ReloadConfig object to enable hot reload, or None to disable hot reload. Cannot be used with workers > 1.
    """

    config = ServerConfig(
        host=host,
        port=port,
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
        _serve_with_reload(import_string, reload, config)
    else:
        app = _import_from_string(application) if isinstance(application, str) else application
        _serve(app, config=config)


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
    sys.path.insert(0, os.getcwd())
    module_str, attr_str = import_string.split(":")
    module = importlib.import_module(module_str)
    return getattr(module, attr_str)


def _serve_with_reload(import_string: str, config: ReloadConfig, server_config: ServerConfig) -> None:
    run_process(
        *config.paths,
        target=_serve_from_import_string,
        args=(import_string,),
        callback=_files_changed_callback,
        kwargs={"config": server_config},
        watch_filter=config.watch_filter,
    )


def _files_changed_callback(file_changes: set[tuple[Change, str]]) -> None:
    changed_files = {f for _, f in file_changes}
    print(f"Files changed: {':'.join(changed_files)}.\nRestarting server...")


def _serve_from_import_string(import_string: str, config: ServerConfig) -> None:
    app = _import_from_string(import_string)
    _serve(app, config=config)


def _serve(
    application: ASGIApplication,
    config: ServerConfig,
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
            addr=[int(i) for i in config.host.split(".")],
            port=config.port,
            keep_alive=config.keep_alive,
            max_concurrency=config.max_concurrency,
            max_size_kb=config.max_size_kb,
            request_timeout=config.request_timeout,
            rate_limit=config.rate_limit,
            buffer_size=config.buffer_size,
            backpressure_timeout=config.backpressure_timeout,
            backpressure_size=config.backpressure_size,
            max_ws_frame_size=config.max_ws_frame_size,
            request_ids=config.request_ids,
            auto_date_header=config.auto_date_header,
            sensitive_headers=config.sensitive_headers,
        )
    )
