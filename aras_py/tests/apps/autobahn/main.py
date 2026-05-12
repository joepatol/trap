import logging

from fastapi import FastAPI, WebSocket


app = FastAPI(debug=True)
server_logger = logging.getLogger("aras")
server_logger.setLevel(logging.INFO)


@app.websocket("/ws")
async def websocket_autobahn_echo(websocket: WebSocket):
    await websocket.accept()
    try:
        while True:
            msg = await websocket.receive()
            if msg["type"] == "websocket.disconnect":
                break
            if msg.get("bytes") is not None:
                await websocket.send_bytes(msg["bytes"])
            else:
                await websocket.send_text(msg["text"])
    except Exception:
        pass
