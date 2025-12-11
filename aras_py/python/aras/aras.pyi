import asyncio
from typing import Awaitable

from .types import CancelToken, ASGIApplication, LogLevel

def generate_cancel_token() -> CancelToken: ...

def serve_python(
    application: ASGIApplication,
    token: CancelToken,
    event_loop: asyncio.AbstractEventLoop,
    addr: list[int] = [127, 0, 0, 1],
    port: int = 8080,
    keep_alive: bool = True,
    log_leveL: LogLevel = "INFO",
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
    timeout_secs: int = 60,
    rate_limit: tuple[int, int] = (1000, 1),
    buffer_size: int = 1024,
    asgi_timeout_secs: int = 10,
) -> Awaitable[None]: ...
