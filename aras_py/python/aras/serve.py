import asyncio
import signal

from .aras import serve_python, generate_cancel_token  # type: ignore
from .types import LogLevel, ASGIApplication


def serve(
    application: ASGIApplication,
    host: str = "127.0.0.1",
    port: int = 8080,
    log_level: LogLevel = "INFO",
    keep_alive: bool = True,
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
    timeout_secs: int = 60,
    rate_limit: tuple[int, int] = (1000, 1),
    buffer_size: int = 1024,
    asgi_timeout_secs: int = 10,
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
            timeout_secs=timeout_secs,
            rate_limit=rate_limit,
            buffer_size=buffer_size,
            asgi_timeout_secs=asgi_timeout_secs,
        )
    )
