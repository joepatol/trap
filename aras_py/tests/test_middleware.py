import asyncio

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio(loop_scope="session")
async def test_body_too_large_returns_413() -> None:
    # Cannot use httpx to reliably test this case because httpx will raise a connection reset error if the server 
    # closes the connection before reading the body.
    # Send headers with Content-Length exceeding the limit but no body.
    # The server rejects based on the header alone (413) before any body is read
    reader, writer = await asyncio.open_connection("127.0.0.1", 8080)
    try:
        writer.write(
            b"POST /stream/large_data HTTP/1.1\r\n"
            b"Host: 127.0.0.1:8080\r\n"
            b"Content-Type: application/octet-stream\r\n"
            b"Content-Length: 1000001\r\n"  # 1 byte over the limit
            b"Connection: close\r\n"
            b"\r\n"
        )
        await writer.drain()
        data = await asyncio.wait_for(reader.read(1024), timeout=5.0)
        assert data.startswith(b"HTTP/1.1 413")
    finally:
        writer.close()
        await asyncio.wait_for(writer.wait_closed(), timeout=2.0)


@pytest.mark.asyncio(loop_scope="session")
async def test_request_timeout_returns_504(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/api/basic/long_task", timeout=15)
    assert response.status_code == 504
    assert response.text == "Gateway Timeout"


@pytest.mark.asyncio(loop_scope="session")
async def test_request_id_present_on_response(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/health_check")
    assert "x-request-id" in response.headers


@pytest.mark.asyncio(loop_scope="session")
async def test_request_ids_are_unique(httpx_client: AsyncClient) -> None:
    r1, r2 = await asyncio.gather(
        httpx_client.get("/health_check"),
        httpx_client.get("/health_check"),
    )
    assert r1.headers["x-request-id"] != r2.headers["x-request-id"]


@pytest.mark.asyncio(loop_scope="session")
async def test_sensitive_header_requests_still_succeed(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get(
        "/health_check",
        headers={"Authorization": "Bearer secret-token"},
    )
    assert response.status_code == 200


@pytest.mark.asyncio(loop_scope="session")
async def test_body_within_limit_is_accepted(httpx_client: AsyncClient) -> None:
    # 500 KB body is well within the 1 MB limit
    data = b"x" * 500_000
    response = await httpx_client.post("/stream/large_data", content=data, timeout=30)
    assert response.status_code == 200
    assert len(response.content) == 500_000


@pytest.mark.asyncio(loop_scope="session")
async def test_auto_date_header_present_by_default(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/health_check")
    assert "Date" in response.headers
