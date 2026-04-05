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
uv lock && maturin develop --uv --extras dev
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

### Document the effective default for `max_concurrency=None`
The Python API accepts `None` for `max_concurrency`, which is mapped to an internal Rust default. Neither the function signature nor the docstring documents what that default is. Users have no way to know what concurrency limit they are running under without reading the Rust source.

### Add a `--workers` CLI option
The CLI has no support for multiple worker processes. Running multiple processes behind a load balancer is the standard production scaling model for Python ASGI apps. Without this option the CLI is not viable as a production deployment tool.

### Simplify the `serve_python` Rust function signature
The Rust function exported to Python takes approximately twelve positional arguments. Adding a new server configuration option requires changes in three places: the Rust function signature, the Python call site, and the CLI. Grouping configuration into a single struct or dict argument would reduce this coupling and make the interface more maintainable.

## Low Priority

### Document hot-reload behaviour on in-flight requests
Hot reload uses `watchfiles.run_process()`, which performs a full process restart on file change. This terminates in-flight requests immediately with no drain period. This behaviour is acceptable for a development feature but should be documented so users are not surprised by abrupt client disconnections during reload.
