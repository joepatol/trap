import asyncio
import websockets

from .conftest import AppContainerInfo


async def send_ws_receive_result(url: str, msg: str) -> str | bytes: 
    async with websockets.connect(url) as ws:
        await ws.send(msg)
        return await ws.recv()


def test_websocket_echo_ok(asgi_application: AppContainerInfo) -> None:
    url = f"{asgi_application.ws_uri}/api/chat/simple"
    result = asyncio.run(send_ws_receive_result(url, "hello"))
    
    assert result == "Message text was: hello"
