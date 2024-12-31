import importlib
import os
import sys

import click
import aras
from aras import LogLevel


@click.group()
def cli() -> None:
    pass


@cli.command()
@click.argument('application', type=click.STRING)
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
def serve(
    application: str,
    host: str,
    port: int,
    log_level: LogLevel,
    no_keep_alive: bool,
    max_concurrency: int | None,
    max_size_kb: int,
) -> None:
    sys.path.insert(0, os.getcwd())
    module_str, application_str = application.split(":")
    try:
        module = importlib.import_module(module_str)
        loaded_app = getattr(module, application_str)
    except Exception as exc:
        raise ImportError(
            "Failed to import ASGI application."
            "Did you provide an import string like 'my_app.main:app'?"
        ) from exc
    aras.serve(
        loaded_app,
        addr=[int(i) for i in host.split(".")],
        port=port,
        log_level=log_level,
        keep_alive=not no_keep_alive,
        max_concurrency=max_concurrency,
        max_size_kb=max_size_kb,
    )
