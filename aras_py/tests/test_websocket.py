import pytest
import websockets
import websockets.exceptions

BASE_URL = "ws://127.0.0.1:8080"


async def send_ws_receive_result(url: str, msg: str) -> str | bytes:
    async with websockets.connect(url) as ws:
        await ws.send(msg)
        return await ws.recv()


@pytest.mark.asyncio(loop_scope="session")
async def test_websocket_echo_ok() -> None:
    url = f"{BASE_URL}/api/chat/ws_echo"
    result = await send_ws_receive_result(url, "hello")
    assert result == "Message text was: hello"


@pytest.mark.asyncio(loop_scope="session")
async def test_websocket_multiple_messages() -> None:
    url = f"{BASE_URL}/api/chat/ws_echo"
    messages = ["first", "second", "third"]
    async with websockets.connect(url) as ws:
        for msg in messages:
            await ws.send(msg)
            result = await ws.recv()
            assert result == f"Message text was: {msg}"


@pytest.mark.asyncio(loop_scope="session")
async def test_websocket_accept_headers() -> None:
    url = f"{BASE_URL}/api/chat/ws_echo"
    async with websockets.connect(url) as ws:
        assert ws.response.headers.get("hello") == "world"


@pytest.mark.asyncio(loop_scope="session")
async def test_websocket_binary_echo() -> None:
    url = f"{BASE_URL}/api/chat/ws_binary_echo"
    payload = b"\x00\x01\x02\x03binary data"
    async with websockets.connect(url) as ws:
        await ws.send(payload)
        result = await ws.recv()
    assert result == payload


@pytest.mark.asyncio(loop_scope="session")
async def test_websocket_server_initiated_close() -> None:
    url = f"{BASE_URL}/api/chat/ws_server_close"
    async with websockets.connect(url) as ws:
        await ws.send("trigger close")
        with pytest.raises(websockets.exceptions.ConnectionClosed):
            await ws.recv()
    assert ws.close_code == 1001


@pytest.mark.asyncio(loop_scope="session")
async def test_websocket_client_disconnect_handled() -> None:
    url = f"{BASE_URL}/api/chat/ws_echo"
    async with websockets.connect(url) as ws:
        await ws.send("before disconnect")
        assert await ws.recv() == "Message text was: before disconnect"
    # Server should continue accepting new connections after client disconnects
    result = await send_ws_receive_result(url, "after disconnect")
    assert result == "Message text was: after disconnect"
