# A Rust ASGI Server

Work in progress!

- Supports http 1.1
- Supports lifespan
- Supports websockets

## Usage

In code

```python
import aras
from fastapi import FastAPI


app = FastAPI()


@app.get("/health_check")
async def root():
    return {"message": "looking good!"}


if __name__ == "__main__":
    aras.serve(app)
```

Or use through CLI

```bash
aras serve my_app.main:app
```

# Testing

To run Rust tests, run `cargo test`.

For Python tests, make sure to build the ARAS docker image.
Run from the project root:

```bash
docker build . -t aras:latest
```

Now run Python tests using `pytest pytests`


To do:

- Cancellation from docker quits python event loop (exiting probably should be done with channel)
- support extensions
- add debug logs
- timeout on waiting from message from ASGI app (what if more_body == true and its never send?)
- Store bytes in `Bytes` iso `Vec`
- Should max_size be an option type?