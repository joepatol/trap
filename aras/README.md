# A Rust ASGI Server

Supports ASGI 3.0 with spec version 2.4

- Supports http 1.1 protocol
- Supports lifespan protocol
- Supports websockets protocol

# Testing

run `cargo test`.

# Improvement Tasks

## High Priority

### Replace `panic!` in `ApplicationHandle` with a returned error
`ApplicationHandle::wait_for_completion` panics if an internal invariant is violated. In async Tokio code, panicking inside a spawned task silently kills that task. This should return an `ArasError` instead so callers can observe and handle the failure.

## Medium Priority

### Use `Arc<State>` in `ScopeFactory`
`ScopeFactory` clones the application state on every request. If the `State` type is non-trivial this becomes a per-request allocation. Wrapping it in `Arc` would allow cheap reference-counted sharing across requests without copying.

### Fix WebSocket close code on clean app exit
When the ASGI application exits without explicitly sending a close frame, the connection is closed with code `1011 Internal Error`. A normal application exit should produce `1000 Normal Closure`. The current behavior misreports clean shutdowns as errors to the client.

### Extract duplicated header conversion in `ScopeFactory`
`build_http` and `build_websocket` contain identical header iteration and `Bytes` conversion code. Any bug fix or optimization must be applied twice. This should be extracted into a shared helper.

## Low Priority

### Document error caching on `ApplicationHandle`
`ApplicationHandle` caches the first error returned by the application task and replays it for all subsequent channel operations. This is a deliberate trade-off for idempotency, but it is undocumented. When debugging failures, the cached error may not reflect the operation that actually triggered the investigation.

### Use `Bytes::slice` in `FrameBuilder` to avoid fragment copies
Each WebSocket frame fragment is built with `Payload::Owned(chunk.to_vec())`, allocating a new heap buffer per fragment. Since `split_bytes` already produces `Bytes` slices into the original buffer, using `Payload::Bytes(chunk)` directly would avoid the copy entirely.

### Remove unnecessary receiver clone in `CommunicationFactory`
The `receive_closure` in `CommunicationFactory::build` clones the channel `Receiver` on every invocation to satisfy the `move` semantics of the inner future. The closure owns the receiver and the clone is not needed — restructuring the closure to mutably borrow it would eliminate the allocation.
