import os
from pathlib import Path

import watchfiles

ASSETS_FOLDER = Path(os.path.dirname(__file__)).parent / ".." / "tests" / "utils" / "application"

def main() -> None:
    for changes in watchfiles.watch("../"):
        print(changes)


if __name__ == "__main__":
    main()
