import asyncio
from typing import Awaitable

from .types import ASGIApplication, CancelToken, LogLevel

def generate_cancel_token() -> CancelToken: ...

def serve_python(
    application: ASGIApplication,
    token: CancelToken,
    event_loop: asyncio.AbstractEventLoop,
    *,
    addr: list[int] = [127, 0, 0, 1],
    port: int = 8080,
    keep_alive: bool = True,
    log_level: LogLevel = "INFO",
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
) -> Awaitable[None]: ...
