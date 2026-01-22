from pathlib import Path

from aras import serve_experimental

here = Path(__file__).parent.resolve()
serve_experimental(
    application="application.main:app",
    application_path=str(here / "utils" / "application" / "main.py"),
    num_workers=2,
)
