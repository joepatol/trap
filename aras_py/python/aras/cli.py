from pathlib import Path

import click

from .serve import ReloadConfig
from .serve import serve as serve_app
from .types import LogLevel


@click.group()
def cli() -> None:
    pass


@cli.command()
@click.argument("application", type=click.STRING)
@click.option(
    "--host",
    type=str,
    default="127.0.0.1",
    help="Bind socket to this host.",
    show_default=True,
)
@click.option(
    "--port",
    type=int,
    default=8080,
    help="Bind socket to this port.",
    show_default=True,
)
@click.option(
    "--log-level",
    type=str,
    default="INFO",
    help="Set the server log level",
    show_default=True,
)
@click.option(
    "--no-keep-alive",
    is_flag=True,
    help="Disable http keep-alive",
)
@click.option(
    "--max-concurrency",
    type=int,
    default=None,
    help="Set the max concurrent requests",
    show_default=True,
)
@click.option(
    "--max-size-kb",
    type=int,
    default=1_000_000,
    help="Set the max size of a request body",
    show_default=True,
)
@click.option(
    "--request-timeout",
    type=int,
    default=180,
    help="Set the request timeout in seconds",
    show_default=True,
)
@click.option(
    "--rate-limit",
    type=(int, int),
    default=(1000, 1),
    help="Set the rate limit as (requests, seconds)",
    show_default=True,
)
@click.option(
    "--buffer-size",
    type=int,
    default=1024,
    help="Set the max number of requests that can be waiting",
    show_default=True,
)
@click.option(
    "--backpressure-timeout",
    type=int,
    default=60,
    help="Number of seconds the server will wait for an expected ASGI event",
    show_default=True,
)
@click.option(
    "--backpressure-size",
    type=int,
    default=16,
    help="Number of pending requests that will trigger backpressure",
    show_default=True,
)
@click.option(
    "--max-ws-frame-size",
    type=int,
    default=64 * 1024,
    help="Set the max size of a single websocket frame in bytes",
    show_default=True,
)
@click.option(
    "--request-ids",
    is_flag=True,
    help="Enable generation and propagation of unique request IDs for each incoming request. The request ID will be included in logs and propagated to the ASGI application via the 'X-Request-ID' header.",
    default=False,
    show_default=True,
)
@click.option(
    "--no-auto-date-header",
    is_flag=True,
    help="Disable automatic addition of the Date header in responses.",
    default=False,
    show_default=True,
)
@click.option(
    "--sensitive-headers",
    type=str,
    multiple=True,
    default=None,
    help="Specify headers that should be treated as sensitive and redacted in logs. Can be used multiple times to specify multiple headers.",
    show_default=True,
)
@click.option(
    "--workers",
    type=int,
    default=1,
    help="Number of worker processes. Each worker runs a full server instance. Cannot be combined with --reload.",
    show_default=True,
)
@click.option(
    "--worker-mode",
    is_flag=True,
    help="Run in worker mode, which is optimized for running multiple workers behind a load balancer. Auto-enabled when --workers > 1.",
    default=False,
)
@click.option(
    "--reload",
    is_flag=True,
    help="Enable hot reload for development. Automatically restarts the server when code changes.",
    default=False,
    show_default=True,
)
@click.option(
    "--reload-path",
    type=str,
    multiple=True,
    default=["."],
    help="Specify paths to watch for changes when hot reload is enabled. Can be used multiple times to specify multiple paths.",
    show_default=True,
)
def serve(
    application: str,
    host: str,
    port: int,
    log_level: LogLevel,
    no_keep_alive: bool,
    max_concurrency: int | None,
    max_size_kb: int,
    request_timeout: int,
    rate_limit: tuple[int, int],
    buffer_size: int,
    backpressure_timeout: int,
    backpressure_size: int,
    max_ws_frame_size: int,
    request_ids: bool,
    no_auto_date_header: bool,
    sensitive_headers: list[str] | None = None,
    workers: int = 1,
    worker_mode: bool = False,
    reload: bool = False,
    reload_path: list[str | Path] = ["."],
) -> None:
    if reload:
        reload_config = ReloadConfig(paths=reload_path)
    else:
        reload_config = None

    serve_app(
        application,
        host=host,
        port=port,
        log_level=log_level,
        keep_alive=not no_keep_alive,
        max_concurrency=max_concurrency,
        max_size_kb=max_size_kb,
        request_timeout=request_timeout,
        rate_limit=rate_limit,
        buffer_size=buffer_size,
        backpressure_timeout=backpressure_timeout,
        backpressure_size=backpressure_size,
        max_ws_frame_size=max_ws_frame_size,
        request_ids=request_ids,
        auto_date_header=not no_auto_date_header,
        sensitive_headers=sensitive_headers,
        workers=workers,
        worker_mode=worker_mode,
        reload=reload_config,
    )
