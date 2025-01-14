from fastapi import APIRouter, Request
from fastapi.responses import StreamingResponse, PlainTextResponse

router = APIRouter()


async def fake_video_streamer():
    for i in range(10):
        yield b"some fake video bytes"


@router.get("/")
async def main() -> StreamingResponse:
    return StreamingResponse(fake_video_streamer())


@router.post("/large_data")
async def large_text(request: Request) -> PlainTextResponse:
    return PlainTextResponse(await request.body())
