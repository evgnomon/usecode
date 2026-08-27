"""Drive the `deploy/compose.yml` stack (podman-compose or docker compose)."""

import json
import subprocess
from pathlib import Path

from .config import Settings

# lib/bot/src/usecode_agent_bot/compose.py -> repo root
_REPO_ROOT = Path(__file__).resolve().parents[4]


class ComposeError(RuntimeError):
    def __init__(self, command: list[str], returncode: int, output: str) -> None:
        super().__init__(
            f"`{' '.join(command)}` failed ({returncode}): {output.strip()}"
        )
        self.command = command
        self.returncode = returncode
        self.output = output


def compose_file(settings: Settings) -> Path:
    if settings.compose_file:
        return Path(settings.compose_file).expanduser().resolve()
    return _REPO_ROOT / "deploy" / "compose.yml"


def compose_command(settings: Settings) -> list[str]:
    path = compose_file(settings)
    if settings.container_cli == "docker":
        return ["docker", "compose", "-f", str(path)]
    return ["podman-compose", "-f", str(path)]


def _run(settings: Settings, *args: str) -> subprocess.CompletedProcess:
    command = [*compose_command(settings), *args]
    try:
        return subprocess.run(
            command,
            capture_output=True,
            text=True,
            cwd=compose_file(settings).parent,
        )
    except FileNotFoundError as exc:
        raise ComposeError(command, -1, str(exc)) from exc


def _check(result: subprocess.CompletedProcess) -> subprocess.CompletedProcess:
    if result.returncode != 0:
        raise ComposeError(
            result.args, result.returncode, result.stderr or result.stdout
        )
    return result


def services(settings: Settings) -> list[str]:
    """All service names defined in the compose file."""
    result = _check(_run(settings, "config", "--services"))
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def _parse_ps_json(text: str) -> list[dict]:
    text = text.strip()
    if not text:
        return []
    try:
        parsed = json.loads(text)
        return parsed if isinstance(parsed, list) else [parsed]
    except json.JSONDecodeError:
        # docker compose emits JSON-lines rather than a single array/object.
        return [json.loads(line) for line in text.splitlines() if line.strip()]


def running_services(settings: Settings) -> list[str]:
    """Names of services from the compose file with a currently running container."""
    result = _check(_run(settings, "ps", "--format", "json"))
    running = []
    for entry in _parse_ps_json(result.stdout):
        state = entry.get("State", "")
        service = entry.get("Service") or entry.get("Labels", {}).get(
            "com.docker.compose.service", ""
        )
        if state == "running" and service:
            running.append(service)
    return running


def is_running(settings: Settings) -> bool:
    """True if every service defined in the compose file has a running container."""
    expected = services(settings)
    return bool(expected) and set(expected) <= set(running_services(settings))


def start(settings: Settings) -> dict:
    """Build and bring the compose stack up in the background.

    `--force-recreate` is required because compose otherwise reuses an
    existing (stopped) container tied to the old image even when `--build`
    produced a newer one under the same tag.
    """
    result = _check(_run(settings, "up", "-d", "--build", "--force-recreate"))
    return {"stdout": result.stdout, "stderr": result.stderr}


def stop(settings: Settings) -> dict:
    """Stop and remove the compose stack's containers."""
    result = _check(_run(settings, "down"))
    return {"stdout": result.stdout, "stderr": result.stderr}


def logs_commands(settings: Settings) -> dict[str, str]:
    """Shell commands the user can run to follow logs for each service."""
    base = " ".join(compose_command(settings))
    commands = {service: f"{base} logs -f {service}" for service in services(settings)}
    commands["all"] = f"{base} logs -f"
    return commands
