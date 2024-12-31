import os
from pathlib import Path
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles
from contextlib import asynccontextmanager
from fastapi.responses import Response, JSONResponse, FileResponse
from fastapi.middleware.cors import CORSMiddleware
from . import db_models
from .database import engine

from . import basic
from . import ws
from . import files
from . import notes
from . import templates
from . import stream

HERE = Path(os.path.dirname(os.path.abspath(__file__)))


@asynccontextmanager
async def lifespan(_: FastAPI):
    db_models.Base.metadata.create_all(bind=engine)
    yield
    db_models.Base.metadata.drop_all(bind=engine)


app = FastAPI(debug=True, lifespan=lifespan)


app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.mount("/static", StaticFiles(directory=str(HERE / "static")), name="static")

app.include_router(basic.router, tags=["Basic"], prefix="/api/basic")
app.include_router(files.router, tags=["Files"], prefix="/api/files")
app.include_router(ws.router, tags=["Websocket"], prefix="/api/chat")
app.include_router(notes.router, tags=["Notes"], prefix="/api/notes")
app.include_router(templates.router, tags=["Templates"], prefix="/site")
app.include_router(stream.router, tags=["Stream responses"], prefix="/stream")


@app.get("/")
async def root() -> Response:
    return Response()


@app.get("/health_check")
async def health_check() -> JSONResponse:
    return JSONResponse({"message": "looking good!"})


@app.get("/favicon.ico")
async def favicon() -> Response:
    return FileResponse(str(HERE / "static/favicon.ico"))
