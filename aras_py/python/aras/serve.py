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
    backpressure_size: int = 16,
    max_ws_frame_size: int = 64 * 1024,
    request_ids: bool = False,
    sensitive_headers: list[str] | None = None,
    hot_reload: bool = False,
) -> None:
    if hot_reload:
        try:
            from watchfiles import DefaultFilter, run_process
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
                backpressure_size,
                max_ws_frame_size,
                request_ids,
                sensitive_headers,
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
            backpressure_size,
            max_ws_frame_size,
            request_ids,
            sensitive_headers,
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
    backpressure_size: int = 16,
    max_ws_frame_size: int = 64 * 1024,
    request_ids: bool = False,
    sensitive_headers: list[str] | None = None,

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
            backpressure_size=backpressure_size,
            max_ws_frame_size=max_ws_frame_size,
            request_ids=request_ids,
            sensitive_headers=sensitive_headers,
        )
    )
