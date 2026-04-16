import multiprocessing
import time
from typing import AsyncGenerator, Generator

import aras
import httpx
import pytest
import pytest_asyncio

from .utils.application.main import app as asgi_app

HOST = "127.0.0.1"
PORT = 8080


def _wait_for_server(host: str, port: int, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    url = f"http://{host}:{port}/health_check"
    while time.monotonic() < deadline:
        try:
            response = httpx.get(url, timeout=1.0)
            if response.status_code == 200:
                return
        except Exception:
            pass
        time.sleep(0.1)
    raise TimeoutError(f"Server at {host}:{port} did not become ready within {timeout}s")


def _run_server_process() -> None:
    aras.serve(
        asgi_app,
        host=HOST,
        port=PORT,
        max_size_kb=1000,
        request_timeout=2,
        rate_limit=(100, 1),
        request_ids=True,
        sensitive_headers=["authorization"],
    )


@pytest.fixture(scope="session", autouse=True)
def run_application_with_server() -> Generator[None, None, None]:
    # Start a new Python process to run the ARAS ASGI server with an
    # application
    process = multiprocessing.Process(
        target=_run_server_process,
        daemon=True,
    )
    process.start()
    _wait_for_server(HOST, PORT)
    yield
    process.terminate()  # Sends SIGTERM
    time.sleep(1)  # Give the server time to shut down
    process.close()


@pytest_asyncio.fixture(scope="session")
async def httpx_client() -> AsyncGenerator[httpx.AsyncClient, None]:
    async with httpx.AsyncClient(base_url=f"http://{HOST}:{PORT}") as client:
        yield client
