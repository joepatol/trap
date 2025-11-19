import asyncio
import signal

from . import aras
from aras import LogLevel, ASGIApplication

def serve(
    application: ASGIApplication,
    host: str,
    port: int,
    log_level: LogLevel = "INFO",
    keep_alive: bool = True,
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
) -> None:
    loop = asyncio.new_event_loop()
    token = aras.generate_cancel_token()

    loop.add_signal_handler(signal.SIGINT, token.stop)
    loop.add_signal_handler(signal.SIGTERM, token.stop)
    
    loop.run_until_complete(
        aras.serve_python(
            application,
            token=token,
            event_loop=loop,
            addr=[int(i) for i in host.split(".")],
            port=port,
            keep_alive=keep_alive,
            log_leveL=log_level,
            max_concurrency=max_concurrency,
            max_size_kb=max_size_kb,
        )
    )
