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


@pytest.fixture(scope="session", autouse=True)
def run_application_with_server() -> Generator[None, None, None]:
    # Start a new Python process to run the ARAS ASGI server with an
    # application
    process= multiprocessing.Process(
        target=_run_server_process,
        daemon=True,
    )
    process.start()
    time.sleep(1)  # Give the server time to start
    yield
    process.terminate()  # Sends SIGTERM
    time.sleep(1)  # Give the server time to shut down
    process.close()


@pytest_asyncio.fixture(scope="session")
async def httpx_client() -> AsyncGenerator[httpx.AsyncClient, None]:
    client = httpx.AsyncClient(base_url=f"http://{HOST}:{PORT}")
    yield client
    await client.aclose()
