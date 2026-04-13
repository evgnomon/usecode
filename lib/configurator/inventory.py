#!/usr/bin/env python3
import json
import logging
import os
from pathlib import Path
import re
import subprocess
from typing import Any

import yaml

logger = logging.getLogger(__name__)


def get_default_gateway() -> str | None:
    try:
        # Check if running in WSL2
        with Path("/proc/version").open("r") as file:
            if "microsoft-standard-WSL2" not in file.read():
                return None

        # Run the 'ip route show' command
        result = subprocess.run(
            ["ip", "route", "show"],
            stdout=subprocess.PIPE,
            check=True,
        )
        output = result.stdout.decode("utf-8")

        # Use regex to find the default gateway IP address
        match = re.search(r"default via (\S+)", output)
        if match:
            return match.group(1)

    except Exception as e:
        logger.info(e)
        return None


def get_inventory() -> dict[str, dict[str, Any]]:
    user_home = os.getenv("HOME")
    user_home = user_home or ""
    if not user_home:
        raise Exception("HOME env variable is not set")
    inventory = {
        "local": {
            "hosts": ["localhost"],
            "vars": {
                "ansible_connection": "local",
            },
        },
    }

    config_file_path = Path(user_home) / ".config" / "blueprint" / "config.yaml"
    if not config_file_path.exists():
        return inventory
    with config_file_path.open() as f:
        configs = yaml.safe_load(f)
        if not configs.get("win_user"):
            return inventory

        host_name = os.getenv("NAME")

        if not host_name:
            return inventory

        if not configs.get("win_user").get(host_name):
            return inventory

        win_home = configs["win_user"][host_name]["home"]
        ansible_python_interpreter = (
            f"{win_home}\\AppData\\Local\\Programs\\Python\\Python312\\python.exe"
        )

        inventory["win"] = {
            "hosts": [get_default_gateway()],
            "vars": {
                "ansible_shell_type": "cmd",
                "ansible_connection": "ssh",
                "ansible_python_interpreter": ansible_python_interpreter,
            },
        }
        return inventory


def main() -> None:
    inventory = get_inventory()
    inventory["_meta"] = {"hostvars": {}}
    print(json.dumps(inventory))


if __name__ == "__main__":
    main()
