import sys
from pathlib import Path

from aras.aras import run_worker

HERE = Path(__file__).parent.parent
WORKER_SCRIPT = HERE / "python" / "aras" / "worker" / "worker.py"

run_worker(sys.executable, str(WORKER_SCRIPT))
