Run the full test suite — Rust unit tests and Python integration tests.

Pass a test file name (e.g. `test_http`) to run only that Python test module.

```bash
cd /Users/joepatol/Documents/dev/trap

# Rust
cargo test

# Python
if [ -n "$ARGUMENTS" ]; then
  PYTHONPATH=aras_py/python aras_py/.venv/bin/pytest aras_py/tests/$ARGUMENTS.py -v
else
  PYTHONPATH=aras_py/python aras_py/.venv/bin/pytest aras_py/tests/ -v
fi
```
