import asyncio
import websockets


async def send_ws_receive_result(url: str, msg: str) -> str | bytes: 
    async with websockets.connect(url) as ws:
        await ws.send(msg)
        return await ws.recv()


def test_websocket_echo_ok() -> None:
    url = f"ws://127.0.0.1:8080/api/chat/simple"
    result = asyncio.run(send_ws_receive_result(url, "hello"))
    
    assert result == "Message text was: hello"
