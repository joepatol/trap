import os
from pathlib import Path

ASSETS_FOLDER = Path(os.path.dirname(__file__)).parent / "assets"

def load_asset(name: str) -> str:
    with open(str(ASSETS_FOLDER / name)) as f:
        return f.read()
