Run all linters — Rust clippy/fmt and Python ruff/isort.

```bash
cd /Users/joepatol/Documents/dev/trap

echo "=== Rust ===" && \
cargo fmt --check && \
cargo clippy -- -D warnings && \

echo "=== Python ===" && \
cd aras_py && \
.venv/bin/ruff check . && \
.venv/bin/isort --check .
```
