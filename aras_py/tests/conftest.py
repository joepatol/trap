import multiprocessing
import time
from typing import AsyncGenerator, Generator

import aras
import httpx
import pytest
import pytest_asyncio

from .apps.protocol.main import app as custom_app
from .apps.protocol.main import no_lifespan_app
from .apps.fastapi.main import app as asgi_app

HOST = "127.0.0.1"
PORT = 8080
CUSTOM_PORT = 8081
NO_LIFESPAN_PORT = 8082


def _wait_for_server(host: str, port: int, path: str = "/health_check", timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    url = f"http://{host}:{port}{path}"
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


def _run_custom_server() -> None:
    aras.serve(custom_app, host=HOST, port=CUSTOM_PORT, request_timeout=3)


def _run_no_lifespan_server() -> None:
    aras.serve(no_lifespan_app, host=HOST, port=NO_LIFESPAN_PORT, request_timeout=3)


@pytest.fixture(scope="session", autouse=True)
def run_application_with_server() -> Generator[None, None, None]:
    process = multiprocessing.Process(target=_run_server_process, daemon=True)
    process.start()
    _wait_for_server(HOST, PORT)
    yield
    process.terminate()
    time.sleep(1)
    process.close()


@pytest.fixture(scope="module")
def custom_server() -> Generator[None, None, None]:
    process = multiprocessing.Process(target=_run_custom_server, daemon=True)
    process.start()
    _wait_for_server(HOST, CUSTOM_PORT, path="/health")
    yield
    process.terminate()
    time.sleep(0.5)
    process.close()


@pytest.fixture(scope="module")
def no_lifespan_server() -> Generator[None, None, None]:
    process = multiprocessing.Process(target=_run_no_lifespan_server, daemon=True)
    process.start()
    _wait_for_server(HOST, NO_LIFESPAN_PORT, path="/")
    yield
    process.terminate()
    time.sleep(0.5)
    process.close()


@pytest_asyncio.fixture(scope="session")
async def httpx_client() -> AsyncGenerator[httpx.AsyncClient, None]:
    async with httpx.AsyncClient(base_url=f"http://{HOST}:{PORT}") as client:
        yield client
