# The Rust ASGI Project

This project consists of 3 packages.

- `asgispec`: ASGI specification expressed in Rust types
- `aras`: an ASGI protocol server
- `aras_py`: Types implementing the `ASGIApplication` trait, usable from Python

The Protocol server can be used from Rust by implementing an application that implements the `ASGIAplication` trait from `asgispec`.

Using the protocol server from Python is done by installing the `aras_py` package and serving a Python ASGI callabe, such as FastAPI.


## Planned

- Implement worker pool for Python workers
- Hot reloading
- Configuration object & configuration through config file
