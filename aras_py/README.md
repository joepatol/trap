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

## Installing

Install using uv

```bash
uv venv .venv && source .venv/bin/activate && maturin develop --uv --extras dev
```

Install using pip

```bash
python3 -m venv .venv && source .venv/bin/activate && pip install .[dev]
```

### Running tests

Now run the tests:

```bash
pytest
```

# Improvement Tasks

## High Priority

### Validate configuration before crossing the FFI boundary
All server configuration is passed from Python to Rust without any upfront checks. Invalid values such as `port=0`, `concurrency_limit=0`, or negative timeouts reach Rust and fail with low-quality error messages. Basic bounds and type validation in the Python layer would produce actionable errors before the server starts.

## Medium Priority

### Simplify the `serve_python` Rust function signature
The Rust function exported to Python takes approximately twelve positional arguments. Adding a new server configuration option requires changes in three places: the Rust function signature, the Python call site, and the CLI. Grouping configuration into a single struct or dict argument would reduce this coupling and make the interface more maintainable.
