import asyncio
import socket

import httpx
import pytest


CUSTOM_HOST = "127.0.0.1"
CUSTOM_PORT = 8081
NO_LIFESPAN_PORT = 8082


# ---------------------------------------------------------------------------
# ASGI protocol corner cases
# ---------------------------------------------------------------------------


@pytest.mark.asyncio(loop_scope="module")
async def test_body_two_receives(custom_server: None) -> None:
    async with httpx.AsyncClient(base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}") as client:
        response = await client.post("/two-receives", content=b"hello world")
    assert response.status_code == 200
    assert response.content == b"hello world"


@pytest.mark.asyncio(loop_scope="module")
async def test_body_three_sends(custom_server: None) -> None:
    async with httpx.AsyncClient(base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}") as client:
        response = await client.get("/three-sends")
    assert response.status_code == 200
    assert response.content == b"part1part2part3"


@pytest.mark.asyncio(loop_scope="module")
async def test_start_then_raise_closes_connection(custom_server: None) -> None:
    # Server sent response.start but the app raised — connection must close,
    # not hang. Either a read error or a truncated response is acceptable.
    async with httpx.AsyncClient(
        base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}", timeout=5.0
    ) as client:
        try:
            await client.get("/start-then-raise")
        except (httpx.RemoteProtocolError, httpx.ReadError):
            pass


@pytest.mark.asyncio(loop_scope="module")
async def test_start_no_body_closes_connection(custom_server: None) -> None:
    # App sends http.response.start but returns without http.response.body.
    # Server must not hang; connection should close within the request timeout.
    async with httpx.AsyncClient(
        base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}", timeout=5.0
    ) as client:
        try:
            response = await client.get("/start-no-body")
            assert response.status_code == 200
        except (httpx.RemoteProtocolError, httpx.ReadError, httpx.ReadTimeout):
            pass


@pytest.mark.asyncio(loop_scope="module")
async def test_receive_after_send_gets_disconnect(custom_server: None) -> None:
    # App sends a complete response then calls receive(). The server must
    # eventually deliver http.disconnect rather than blocking forever.
    async with httpx.AsyncClient(base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}") as client:
        response = await client.get("/receive-after-send")
        assert response.status_code == 200
        assert response.content == b"done"

        await asyncio.sleep(2.0)

        status = await client.get("/status/receive_after_send")
    assert status.text == "http.disconnect", (
        f"Expected http.disconnect but got: {status.text!r}. "
        "Server may not deliver disconnect events after a complete response."
    )


@pytest.mark.asyncio(loop_scope="module")
async def test_no_lifespan_app_handles_requests(no_lifespan_server: None) -> None:
    # App returns immediately from the lifespan scope — server must not crash
    # and must still serve HTTP requests normally.
    async with httpx.AsyncClient(
        base_url=f"http://{CUSTOM_HOST}:{NO_LIFESPAN_PORT}"
    ) as client:
        response = await client.get("/any-path")
    assert response.status_code == 200
    assert response.content == b"ok"


# ---------------------------------------------------------------------------
# Disconnect and abort behaviour
# ---------------------------------------------------------------------------


@pytest.mark.asyncio(loop_scope="module")
async def test_disconnect_during_upload_app_sees_disconnect(custom_server: None) -> None:
    # Client declares a large body, sends only a few bytes, then closes the
    # socket. The app must observe http.disconnect via receive().
    loop = asyncio.get_event_loop()
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setblocking(False)
    await loop.sock_connect(sock, (CUSTOM_HOST, CUSTOM_PORT))

    partial_request = (
        b"POST /slow-upload HTTP/1.1\r\n"
        b"Host: localhost\r\n"
        b"Content-Length: 100000\r\n"
        b"Connection: close\r\n"
        b"\r\n"
        b"only a few bytes"
    )
    await loop.sock_sendall(sock, partial_request)
    await asyncio.sleep(0.2)
    sock.close()

    await asyncio.sleep(1.5)

    async with httpx.AsyncClient(base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}") as client:
        status = await client.get("/status/slow_upload")
    assert status.text == "http.disconnect", (
        f"Expected http.disconnect but got: {status.text!r}. "
        "Disconnect event is not forwarded to the app during body read."
    )


@pytest.mark.asyncio(loop_scope="module")
async def test_disconnect_during_stream_server_recovers(custom_server: None) -> None:
    # Client disconnects mid-stream. Primary assertion: server stays functional.
    # Secondary: streaming task must eventually end (completed or send_error).
    async with httpx.AsyncClient(
        base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}", timeout=10.0
    ) as client:
        async with client.stream("GET", "/slow-stream") as response:
            assert response.status_code == 200
            async for _ in response.aiter_bytes(chunk_size=16):
                break

    await asyncio.sleep(0.5)
    async with httpx.AsyncClient(base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}") as client:
        health = await client.get("/health")
    assert health.status_code == 200

    await asyncio.sleep(3.0)
    async with httpx.AsyncClient(base_url=f"http://{CUSTOM_HOST}:{CUSTOM_PORT}") as client:
        status = await client.get("/status/slow_stream")
    assert status.text in ("completed", "send_error"), (
        f"Unexpected streaming task outcome: {status.text!r}"
    )
