import asyncio
import signal

from .aras import generate_cancel_token, serve_python  # type: ignore
from .types import ASGIApplication, LogLevel


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
    hot_reload: bool = False,
) -> None:
    if hot_reload:
        try:
            from watchfiles import run_process, DefaultFilter
        except ImportError:
            raise ImportError(
                "watchfiles is required for hot reload. Please install it with 'pip install aras_py[hot-reload]'."
            )

        run_process(
            ".",
            target=_serve,
            args=(
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
            ),
            watch_filter=DefaultFilter(),
        )
    else:
        _serve(
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
    max_ws_frame_size: int = 64 * 1024,
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
