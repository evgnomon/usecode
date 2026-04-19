from pathlib import Path
from invoke import task

ROOT = Path(__file__).parent
VENV = ROOT / ".venv"
BIN = VENV / "bin"
PY = BIN / "python"
PIP = BIN / "pip"


@task
def venv(c):
    """Create .venv if missing."""
    if not PY.exists():
        c.run(f"python3 -m venv {VENV}")


@task(pre=[venv])
def install(c):
    """Install dependencies into .venv."""
    c.run(f"{PIP} install -q -r {ROOT / 'requirements.txt'}")


@task(pre=[install])
def serve(c, host="127.0.0.1", port=8000, reload=True):
    """Run the FastAPI app with uvicorn."""
    flag = "--reload" if reload else ""
    c.run(f"{BIN}/uvicorn main:app --host {host} --port {port} {flag}", pty=True)


@task
def clean(c):
    """Remove .venv and Python caches."""
    c.run(f"rm -rf {VENV} __pycache__ */__pycache__")
