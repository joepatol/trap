from typing import Awaitable

from asyncio import BaseEventLoop
from .aras_types import ASGIApplication, LogLevel

class CancelToken:
    def stop(self) -> None: ...


def generate_cancel_token() -> CancelToken: ...


def serve_python(
    application: ASGIApplication,
    token: CancelToken,
    event_loop: BaseEventLoop,
    addr: list[int] = [127, 0, 0, 1],
    port: int = 8080,
    keep_alive: bool = True,
    log_leveL: LogLevel = "INFO",
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
) -> Awaitable[None]: ...
