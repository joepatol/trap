# A Rust ASGI Server for Python

This Python package uses the ARAS Rust server to serva a Python ASGI application.

## Serving an ASGI application

Serving is done through code:

```python
import aras
import FastAPI
from fastapi.responses import PlainTextResponse

app = FastAPI()

@app.get("/")
async def hello() -> PlainTextResponse:
    return PlainTextResponse("Hello world!")


if __name__ == "__main__":
    aras.serve(app)
```

Or you can run you application through the ARAS cli:

```bash
aras serve my_app.main:app
```

## Running tests

Install the project from the aras_py root

```bash
python3 -m venv .venv && source .venv/bin/activate && pip install .[dev]
```

Now run the tests:

```bash
pytest
```
