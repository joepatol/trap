# Types and traits for working with ASGI in Rust

# Improvement Tasks

## High Priority

### Remove `Display` bound from `State` trait
`State: Display` forces every user-defined state type to implement `Display`, even though the bound is only needed when logging a `Scope`. This should be moved to use-site bounds (e.g. `where S: State + Serialize`) so application authors are not penalised for state types without a natural string representation.

### Add error channel to `ReceiveFn`
`ReceiveFuture` currently resolves to `ASGIReceiveEvent` with no error path. The convention of synthesising a disconnect event on failure is implicit and undocumented. The return type should be `Result<ASGIReceiveEvent, DisconnectedClient>` to make the contract explicit and match the symmetry of `SendFn`.

### Replace `WebsocketSendEvent` / `WebsocketReceiveEvent` dual-optional fields with a typed payload enum
Both events expose `bytes: Option<Bytes>` and `text: Option<String>`, making it possible to construct a value where both fields are `None` (an empty frame) or both are `Some` (ambiguous). A `WebsocketPayload { Text(String), Binary(Bytes) }` enum makes only valid states representable.

## Medium Priority

### Add validation to constructors
HTTP status codes, WebSocket close codes, and header values are accepted as raw `u16` or `Bytes` with no bounds checking. Constructors should return `Result` or use validated newtypes so malformed values are rejected at construction rather than silently reaching the wire.

### Drop `Unpin + Sync` bounds from `SendFuture` / `ReceiveFuture`
`Sync` is unnecessary for futures that execute on a single task and rules out common patterns such as holding a `Mutex` guard across an await point. Only `Send + Unpin` are required.

### Add `DisconnectedClient` context field
`DisconnectedClient` is a unit struct, so when a send fails mid-stream the error carries no information for log correlation. An optional message or request-id field would make failures actionable.

## Low Priority

### Add documentation to all public items
None of the public types, traits, or fields carry doc comments. As the API contract between server and application, `asgispec` is the primary surface application authors interact with. Every scope field, event type, and trait method should have at least a one-line doc explaining its role and, where relevant, the ASGI spec section it corresponds to.

### Consider a `Sender` / `Receiver` trait instead of `Arc<dyn Fn>` type aliases
`SendFn` / `ReceiveFn` are `Arc<dyn Fn>` aliases. This works but allocates on every call and is harder to mock in tests than a named trait. A `Sender` / `Receiver` trait (analogous to `aras`'s internal `SendToASGIApp` / `ReceiveFromASGIApp`) would be more ergonomic and testable.

### Model ASGI extension dictionaries
The ASGI spec includes optional extension slots (e.g. `http.response.push`, `http.response.trailers`). There is currently no extension field, meaning the types cannot be extended without a breaking API change. Adding an `extensions: Option<HashMap<String, serde_json::Value>>` field to the relevant scopes and events would future-proof the interface.

### Expand test coverage
Only two tests exist, both checking JSON serialization shape. Roundtrip deserialization tests, construction tests, and at least one end-to-end test of a minimal `ASGIApplication` implementation would meaningfully increase confidence in the spec contract.