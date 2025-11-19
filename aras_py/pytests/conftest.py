import time
import multiprocessing
from typing import AsyncGenerator, Generator

import pytest
import httpx
import pytest_asyncio
import aras

from .utils.application.main import app as asgi_app


HOST = "127.0.0.1"
PORT = 8080


@pytest.fixture(scope="session")
def run_application_with_server() -> Generator[None, None, None]:
    process= multiprocessing.Process(
        target=_run_server_process,
        daemon=True,
    )
    process.start()
    time.sleep(1)  # Give the server time to start
    yield
    process.terminate()
    time.sleep(1)  # Give the server time to shut down
    process.close()


@pytest_asyncio.fixture(scope="session")
async def httpx_client(run_application_with_server) -> AsyncGenerator[httpx.AsyncClient, None]:
    client = httpx.AsyncClient(base_url=f"http://{HOST}:{PORT}")
    yield client


def _run_server_process() -> None:
    aras.serve(
        asgi_app,
        host=HOST,
        port=PORT,
        log_level="INFO",
        no_keep_alive=False,
        max_concurrency=None,
        max_size_kb=1_000_000,
    )
