from typing import Any
import asyncio

from fastapi import APIRouter, Request, Response
from fastapi.responses import JSONResponse, PlainTextResponse

router = APIRouter()


@router.get("/echo_text")
async def echo(data: str) -> PlainTextResponse:
    return PlainTextResponse(data)


@router.post("/echo_json")
async def echo_json(data: dict[str, Any]) -> JSONResponse:
    return JSONResponse(data)


@router.get("/long_task")
async def long_task() -> JSONResponse:
    await asyncio.sleep(20.0)
    return JSONResponse({"task": "done"})


@router.get("/more_headers")
async def more_headers() -> PlainTextResponse:
    return PlainTextResponse(headers={"the": "header"})


@router.get("/error", status_code=500)
async def error() -> JSONResponse:
    raise ValueError("This is an error")


@router.patch("/state", summary="Update the server ASGI state", status_code=204)
async def use_request(data: dict[str, Any], request: Request) -> Response:
    for k, v in data.items():
        setattr(request.state, k, v)
    return Response(status_code=204)


@router.get("/state", summary="Get the server ASGI state")
async def get_state(request: Request) -> PlainTextResponse:
    return PlainTextResponse(str(request.state._state))


fake_items_db = [{"item_name": "Foo"}, {"item_name": "Bar"}, {"item_name": "Baz"}]


@router.get("/items/")
async def read_item(skip: int = 0, limit: int = 10):
    return fake_items_db[skip : skip + limit]
