Set up the development environment or build a release wheel.

Pass `release` to build a distributable wheel. No argument installs the package with all dev dependencies.

```bash
cd /Users/joepatol/Documents/dev/trap/aras_py

if [ "$ARGUMENTS" = "release" ]; then
  .venv/bin/pip wheel . --no-deps -w dist/
else
  .venv/bin/pip install -e ".[dev]"
fi
```
