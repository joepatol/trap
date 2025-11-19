import signal
import asyncio

from aras.aras_types import ASGIApplication, LogLevel
from aras_core.aras_core import generate_cancel_token, serve_python


def serve(
    application: ASGIApplication,
    host: str = "127.0.0.1",
    port: int = 8080,
    log_level: LogLevel = "INFO",
    keep_alive: bool = True,
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
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
        )
    )
