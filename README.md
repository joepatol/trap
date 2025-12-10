# The Rust ASGI Project


Todo list:
- Finish rust tests
- Python tests
- ASGI extensions
- add debug logs
- timeout on waiting from message from ASGI app (what if more_body == true and its never send?) -> should be solved by timeout layer
- Store bytes in `Bytes` iso `Vec`
- Should max_size be an option type?