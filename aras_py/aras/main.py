import asyncio
import signal

from . import aras
from aras import LogLevel, ASGIApplication

def serve(
    application: ASGIApplication,
    host: str,
    port: int,
    log_level: LogLevel,
    no_keep_alive: bool,
    max_concurrency: int | None,
    max_size_kb: int,
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
            keep_alive=not no_keep_alive,
            max_concurrency=max_concurrency,
            max_size_kb=max_size_kb,
        )
    )
