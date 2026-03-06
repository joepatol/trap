import importlib
import os
import sys

import click

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
    "--max-ws-frame-size",
    type=int,
    default=64 * 1024,
    help="Set the max size of a single websocket frame in bytes",
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
    max_ws_frame_size: int,
) -> None:
    # Insert current working directory to sys.path to make sure the dynamic import,
    # which is referenced from the cwd, works correctly.
    sys.path.insert(0, os.getcwd())
    module_str, application_str = application.split(":")

    try:
        module = importlib.import_module(module_str)
        loaded_app = getattr(module, application_str)
    except Exception as exc:
        raise ImportError(
            "Failed to import ASGI application."
            "Did you provide an import string like 'my_app.main:app'?"
            "Make sure you provided a valid path from the current working directory."
        ) from exc

    serve_app(
        loaded_app,
        host,
        port,
        log_level,
        not no_keep_alive,
        max_concurrency,
        max_size_kb,
        request_timeout,
        rate_limit,
        buffer_size,
        backpressure_timeout,
        max_ws_frame_size,
    )
