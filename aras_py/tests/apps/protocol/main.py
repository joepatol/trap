import asyncio

_observations: dict[str, str] = {}


async def app(scope, receive, send):
    if scope["type"] == "lifespan":
        event = await receive()
        if event["type"] == "lifespan.startup":
            await send({"type": "lifespan.startup.complete"})
        event = await receive()
        if event["type"] == "lifespan.shutdown":
            await send({"type": "lifespan.shutdown.complete"})
        return

    path = scope["path"]
    handlers = {
        "/two-receives": _two_receives,
        "/three-sends": _three_sends,
        "/start-then-raise": _start_then_raise,
        "/start-no-body": _start_no_body,
        "/receive-after-send": _receive_after_send,
        "/slow-upload": _slow_upload,
        "/slow-stream": _slow_stream,
    }
    if path in handlers:
        await handlers[path](scope, receive, send)
    elif path.startswith("/status/"):
        key = path[len("/status/"):]
        value = _observations.get(key, "not_observed").encode()
        await send({"type": "http.response.start", "status": 200, "headers": []})
        await send({"type": "http.response.body", "body": value, "more_body": False})
    elif path == "/health":
        await send({"type": "http.response.start", "status": 200, "headers": []})
        await send({"type": "http.response.body", "body": b"ok", "more_body": False})
    else:
        await send({"type": "http.response.start", "status": 404, "headers": []})
        await send({"type": "http.response.body", "body": b"not found", "more_body": False})


async def no_lifespan_app(scope, _receive, send):
    if scope["type"] == "lifespan":
        return  # Does not implement lifespan — just returns immediately
    await send({"type": "http.response.start", "status": 200, "headers": []})
    await send({"type": "http.response.body", "body": b"ok", "more_body": False})


async def _two_receives(scope, receive, send):
    event1 = await receive()
    body = event1.get("body", b"")
    if event1.get("more_body", False):
        event2 = await receive()
        body += event2.get("body", b"")
    await send({"type": "http.response.start", "status": 200, "headers": []})
    await send({"type": "http.response.body", "body": body, "more_body": False})


async def _three_sends(scope, receive, send):
    await receive()
    await send({"type": "http.response.start", "status": 200, "headers": []})
    await asyncio.sleep(0.01)
    await send({"type": "http.response.body", "body": b"part1", "more_body": True})
    await asyncio.sleep(0.01)
    await send({"type": "http.response.body", "body": b"part2", "more_body": True})
    await asyncio.sleep(0.01)
    await send({"type": "http.response.body", "body": b"part3", "more_body": False})


async def _start_then_raise(scope, receive, send):
    await send({"type": "http.response.start", "status": 200, "headers": []})
    raise RuntimeError("deliberate error after response start")


async def _start_no_body(scope, receive, send):
    await send({"type": "http.response.start", "status": 200, "headers": []})
    # Returns without sending http.response.body


async def _receive_after_send(scope, receive, send):
    await receive()
    await send({"type": "http.response.start", "status": 200, "headers": []})
    await send({"type": "http.response.body", "body": b"done", "more_body": False})
    try:
        event = await asyncio.wait_for(receive(), timeout=5.0)
        _observations["receive_after_send"] = event["type"]
    except asyncio.TimeoutError:
        _observations["receive_after_send"] = "timeout"


async def _slow_upload(scope, receive, send):
    while True:
        event = await receive()
        if event["type"] == "http.disconnect":
            _observations["slow_upload"] = "http.disconnect"
            return
        if not event.get("more_body", False):
            _observations["slow_upload"] = "completed"
            await send({"type": "http.response.start", "status": 200, "headers": []})
            await send({"type": "http.response.body", "body": b"ok", "more_body": False})
            return
        await asyncio.sleep(0.05)


async def _slow_stream(scope, receive, send):
    await receive()
    await send({"type": "http.response.start", "status": 200, "headers": []})
    try:
        for i in range(20):
            await asyncio.sleep(0.15)
            await send({
                "type": "http.response.body",
                "body": f"chunk{i}\n".encode(),
                "more_body": i < 19,
            })
        _observations["slow_stream"] = "completed"
    except Exception:
        _observations["slow_stream"] = "send_error"
