import asyncio
import importlib
import multiprocessing
import os
import signal
import sys
from pathlib import Path
from dataclasses import dataclass

from watchfiles import BaseFilter, run_process
from watchfiles.main import FileChange

from .aras import generate_cancel_token, serve_python  # type: ignore
from .types import ASGIApplication, LogLevel


@dataclass
class ReloadConfig:
    paths: list[str | Path] = "."
    watch_filter: BaseFilter | None = None


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
    workers: int = 1,
    reload: ReloadConfig | None = None,
) -> None:
    if workers > 1 and reload:
        raise ValueError("Cannot use both 'workers' and 'reload' at the same time")

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

    if workers > 1:
        import_string = _resolve_import_string(application)
        _serve_with_workers(import_string, workers, **kwargs)
    elif reload:
        import_string = _resolve_import_string(application)
        _serve_with_reload(import_string, reload, **kwargs)
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
    sys.path.insert(0, os.getcwd())
    module_str, attr_str = import_string.split(":")
    module = importlib.import_module(module_str)
    return getattr(module, attr_str)


def _serve_with_workers(import_string: str, workers: int, **kwargs) -> None:  # type: ignore[no-untyped-def]
    processes: list[multiprocessing.Process] = []
    for _ in range(workers):
        p = multiprocessing.Process(
            target=_serve_from_import_string,
            args=(import_string,),
            kwargs={**kwargs, "worker_mode": True},
        )
        p.start()
        processes.append(p)

    def _terminate_workers(signum: int, frame: object) -> None:
        for p in processes:
            p.terminate()

    signal.signal(signal.SIGTERM, _terminate_workers)
    signal.signal(signal.SIGINT, _terminate_workers)

    try:
        for p in processes:
            p.join()
    except KeyboardInterrupt:
        for p in processes:
            p.terminate()
        for p in processes:
            p.join()


def _serve_with_reload(import_string: str, config: ReloadConfig, **kwargs) -> None:  # type: ignore[no-untyped-def]
    run_process(
        *config.paths,
        target=_serve_from_import_string,
        args=(import_string,),
        callback=_files_changed_callback,
        kwargs=kwargs,
        watch_filter=config.watch_filter,
    )


def _files_changed_callback(file_changes: set[tuple[FileChange, str]]) -> None:
    changed_files = {f for _, f in file_changes}
    print(f"Files changed: {':'.join(changed_files)}.\nRestarting server...")


def _serve_from_import_string(import_string: str, worker_mode: bool = False, **kwargs) -> None:  # type: ignore[no-untyped-def]
    app = _import_from_string(import_string)
    _serve(app, worker_mode=worker_mode, **kwargs)


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
    worker_mode: bool = False,
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
            worker_mode=worker_mode,
        )
    )
