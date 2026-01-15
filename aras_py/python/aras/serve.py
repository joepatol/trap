import asyncio
import signal
import logging
from multiprocessing import Process

import watchfiles

from .aras import serve_python, generate_cancel_token  # type: ignore
from .types import LogLevel, ASGIApplication
from .supervisor import ReloadSupervisor

logger = logging.getLogger("aras.serve")


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
        _run_hot_reload(
            application,
            host,
            port,
            log_level,
            keep_alive,
            max_concurrency,
            max_size_kb,
            request_timeout,
            rate_limit,
            buffer_size,
            backpressure_timeout,
            max_ws_frame_size,
        )
    else:
        _run_one(
            application,
            host,
            port,
            log_level,
            keep_alive,
            max_concurrency,
            max_size_kb,
            request_timeout,
            rate_limit,
            buffer_size,
            backpressure_timeout,
            max_ws_frame_size,
        )


def _run_one(
    application: ASGIApplication,  
    host: str,
    port: int,
    log_level: LogLevel,
    keep_alive: bool,
    max_concurrency: int | None,
    max_size_kb: int,
    request_timeout: int,
    rate_limit: tuple[int, int],
    buffer_size: int,
    backpressure_timeout: int,
    max_ws_frame_size: int,
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
            max_ws_frame_size=max_ws_frame_size,
        )
    )


def _run_hot_reload(
    application: ASGIApplication,
    host: str,
    port: int,
    log_level: LogLevel,
    keep_alive: bool,
    max_concurrency: int | None,
    max_size_kb: int,
    request_timeout: int,
    rate_limit: tuple[int, int],
    buffer_size: int,
    backpressure_timeout: int,
    max_ws_frame_size: int,
) -> None:
    def _spawn_process() -> Process:
        return Process(
            target=_run_one,
            daemon=True,
            kwargs={
                "application": application,
                "host": host,
                "port": port,
                "log_level": log_level,
                "keep_alive": keep_alive,
                "max_concurrency": max_concurrency,
                "max_size_kb": max_size_kb,
                "request_timeout": request_timeout,
                "rate_limit": rate_limit,
                "buffer_size": buffer_size,
                "backpressure_timeout": backpressure_timeout,
                "max_ws_frame_size": max_ws_frame_size,
            },
        )

    supervisor = ReloadSupervisor(_spawn_process)

    location = application.__module__
    print(f"Watching for changes in {location}")

    for changes in watchfiles.watch(str(location), stop_event=supervisor.should_exit):
        print("Changes detected: %s", changes)
        print("Restarting server...")
        try:
            supervisor.restart()
        except Exception as e:
            print("Error while restarting server: %s", e)
