"""Run the AI model (llama-server) as a container on the usecode-agent-api host."""

import json
import subprocess
from pathlib import Path

from .config import Settings
from .model_config import resolve

CONTAINER_NAME = "usecode-agent-model"
INTERNAL_PORT = 8080

# Local image built from MODEL_CONTAINERFILE, layering extra tooling (curl,
# bash, procps, ...) on top of whichever upstream llama.cpp image is
# selected. Rebuilt every time the model is started so it always tracks the
# chosen base image.
MODEL_IMAGE = "usecode-agent-model:local"
MODEL_CONTAINERFILE = Path(__file__).resolve().parent / "model_image" / "Containerfile"


class ModelProcessError(RuntimeError):
    def __init__(self, command: list[str], returncode: int, output: str) -> None:
        super().__init__(
            f"`{' '.join(command)}` failed ({returncode}): {output.strip()}"
        )
        self.command = command
        self.returncode = returncode
        self.output = output


def _run(settings: Settings, *args: str) -> subprocess.CompletedProcess:
    command = [settings.model_container_cli, *args]
    try:
        return subprocess.run(command, capture_output=True, text=True)
    except FileNotFoundError as exc:
        raise ModelProcessError(command, -1, str(exc)) from exc


def _check(result: subprocess.CompletedProcess) -> subprocess.CompletedProcess:
    if result.returncode != 0:
        raise ModelProcessError(
            result.args, result.returncode, result.stderr or result.stdout
        )
    return result


def _build_args(config: dict) -> list[str]:
    return [
        "build",
        "--build-arg",
        f"BASE_IMAGE={config['image']}",
        "-t",
        MODEL_IMAGE,
        "-f",
        str(MODEL_CONTAINERFILE),
        str(MODEL_CONTAINERFILE.parent),
    ]


def _run_args(config: dict) -> list[str]:
    args = [
        "run",
        "-d",
        "--replace",
        "--name",
        CONTAINER_NAME,
        "-p",
        f"{config['host']}:{config['port']}:{INTERNAL_PORT}",
    ]
    if "vulkan" in str(config["device"]).lower():
        args += ["--device", "/dev/dri"]
    args += [MODEL_IMAGE]
    args += [
        "-hf",
        config["hf_repo"],
        "--device",
        config["device"],
        "-ngl",
        str(config["ngl"]),
        "--alias",
        config["alias"],
        "-c",
        str(config["ctx_size"]),
        "--host",
        "0.0.0.0",
        "--port",
        str(INTERNAL_PORT),
    ]
    return args


def start(settings: Settings, overrides: dict) -> dict:
    """Resolve overrides, (re)build the model image, and (re)start the container."""
    config = resolve(overrides)
    build_result = _check(_run(settings, *_build_args(config)))
    run_result = _check(_run(settings, *_run_args(config)))
    return {
        "config": config,
        "stdout": run_result.stdout,
        "stderr": run_result.stderr,
        "build_stdout": build_result.stdout,
        "build_stderr": build_result.stderr,
    }


def stop(settings: Settings) -> dict:
    """Stop and remove the model container. No-op if it isn't running."""
    result = _run(settings, "rm", "-f", CONTAINER_NAME)
    if result.returncode != 0 and "no such container" not in (
        result.stderr or ""
    ).lower():
        raise ModelProcessError(result.args, result.returncode, result.stderr)
    return {"stdout": result.stdout, "stderr": result.stderr}


def status(settings: Settings) -> dict:
    """Current running state of the model container, if any."""
    result = _run(
        settings, "inspect", CONTAINER_NAME, "--format", "{{json .State}}"
    )
    if result.returncode != 0:
        return {"running": False, "config": None, "state": None}
    try:
        state = json.loads(result.stdout.strip() or "{}")
    except json.JSONDecodeError:
        state = {}
    return {"running": bool(state.get("Running")), "config": None, "state": state}
