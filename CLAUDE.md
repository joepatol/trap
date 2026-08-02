# Trap — Rust ASGI Server

## Project layout

| Crate | Purpose |
|-------|---------|
| `asgispec/` | ASGI protocol types expressed in Rust types & traits |
| `aras/` | Core protocol server (hyper 1 + tower + tokio) |
| `aras_py/` | PyO3/maturin Python bindings (published as the `aras` PyPI package) |

## Build

```bash
# Install package + all dev dependencies (editable)
cd aras_py && .venv/bin/pip install -e ".[dev]"

# Build a release wheel
cd aras_py && .venv/bin/pip wheel . --no-deps -w dist/

# Rust crates only
cargo build
```

## Testing

```bash
# Rust unit tests (all crates)
cargo test

# Python integration tests
cd aras_py && PYTHONPATH=python .venv/bin/pytest tests/ -v

# Single file
cd aras_py && PYTHONPATH=python .venv/bin/pytest tests/test_http.py -v
```

## Linting / formatting

```bash
# Python (from aras_py/)
.venv/bin/ruff check . && .venv/bin/isort --check .

# Rust
cargo clippy -- -D warnings
cargo fmt --check
```

## Code conventions

- **Rust**: no `unwrap()` in library code — use `?` with `thiserror`-derived error types.
- **Rust**: use the `ArasResult<T>` alias, not raw `Result<T, E>`.
- **Rust**: tracing macros (`info!`, `error!`, etc.) for all observability; no `println!`.
- **Python bindings**: Rust↔Python type conversions live in `aras_py/src/convert.rs`; keep that file focused on conversion, not business logic.
- **Tower middleware**: new layers go in `aras/src/layers/`. Wire them into the stack in `server.rs`, respecting the documented ordering in that file's module-level doc comment.

## Performance baseline (2026-04-03, M5 MacBook Pro)

| Server | RPS |
|--------|-----|
| uvicorn | ~6 700 |
| aras | ~8 200 (+22 %) |

Changes to hot paths (`service.rs`, `protocols/`) should be benchmarked against these numbers before merging.
