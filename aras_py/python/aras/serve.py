import sys
import os
import asyncio
import signal
from pathlib import Path

from .aras import serve_in_process, serve_with_workers, generate_cancel_token  # type: ignore
from .types import LogLevel, ASGIApplication


def serve_experimental(
    application: str,
    application_path: str,
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
    max_ws_frame_size: int = 64 * 1024,
    reload: bool = False,
    num_workers: int = 2,
) -> None:
    if reload:
        raise NotImplementedError("Auto-reload is not fully implemented yet.")
    
    root = Path(application_path).resolve()
    cur_python_path = os.environ.get("PYTHONPATH", "")
    if cur_python_path:
        pythonpath = f"{cur_python_path}:{root.parent}:{root.parent.parent}"
    else:
        pythonpath = f"{root.parent}:{root.parent.parent}"

    token = generate_cancel_token()
    signal.signal(signal.SIGINT, lambda _, __: token.stop())
    signal.signal(signal.SIGTERM, lambda _, __: token.stop())

    cur_dir = Path(__file__).parent
    worker_script = cur_dir / "worker" / "worker.py"

    serve_with_workers(
        application,
        pythonpath,
        sys.executable,
        str(worker_script),
        token,
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
        max_ws_frame_size=max_ws_frame_size,
        num_workers=num_workers,
    )


def serve(
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
    max_ws_frame_size: int = 64 * 1024,
    reload: bool = False,
) -> None:
    if reload:
        raise NotImplementedError("Auto-reload is not fully implemented yet.")
    
    loop = asyncio.new_event_loop()
    token = generate_cancel_token()

    loop.add_signal_handler(signal.SIGINT, token.stop)
    loop.add_signal_handler(signal.SIGTERM, token.stop)
    
    loop.run_until_complete(
        serve_in_process(
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
            max_ws_frame_size=max_ws_frame_size,
        )
    )
