from pprint import pprint
from aras import serve
from aras.types import Scope, Receive, Send


async def lifespan(_: Scope, receive: Receive, send: Send):
    while True:
        message = await receive()
        pprint(message)
        
        if message["type"] == "lifespan.startup":
            await send({"type": "lifespan.startup.complete"})
        
        elif message["type"] == "lifespan.shutdown":
            await send({"type": "lifespan.shutdown.complete"})
            return


async def http(scope: Scope, receive: Receive, send: Send):
    while True:
        message = await receive()
        pprint(message)

        if message["type"] == "http.disconnect":
            return

        if message["type"] == "http.request":
            body = b"Hello, World!"
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [(b"content-type", b"text/plain")],
                }
            )
            await send({
                "type": "http.response.body",
                "body": body,
                "more_body": False,
            })


async def websocket(scope: Scope, receive: Receive, send: Send):
    while True:
        message = await receive()
        pprint(message)

        if message["type"] == "websocket.connect":
            await send({"type": "websocket.accept"})
        
        elif message["type"] == "websocket.receive":
            if message["text"] is not None:
                await send({
                    "type": "websocket.send",
                    "text": f"Echo: {message['text']}",
                })
            else:
                await send({
                    "type": "websocket.send",
                    "bytes": message["bytes"],
                })
        
        elif message["type"] == "websocket.disconnect":
            return


async def asgi_application(scope: Scope, receive: Receive, send: Send):
    pprint(scope)
    if scope["type"] == "lifespan":
        await lifespan(scope, receive, send)
    elif scope["type"] == "http":
        await http(scope, receive, send)
    elif scope["type"] == "websocket":
        await websocket(scope, receive, send)
    else:
        raise RuntimeError("Unsupported scope type")


if __name__ == "__main__":
    serve(asgi_application, backpressure_timeout=5)
