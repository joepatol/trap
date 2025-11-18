import os
from pathlib import Path

from fastapi import APIRouter, Request
from fastapi.templating import Jinja2Templates
from fastapi.responses import HTMLResponse

HERE = Path(os.path.dirname(os.path.abspath(__file__)))


router = APIRouter()


templates = Jinja2Templates(directory=str(HERE / "templates"))


@router.get("/items/{id}", response_class=HTMLResponse)
async def read_item(request: Request, id: str):
    return templates.TemplateResponse(
        request=request, name="item.html", context={"id": id}
    )
