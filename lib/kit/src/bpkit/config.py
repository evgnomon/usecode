from pathlib import Path
from typing import Optional, Union

import yaml
from pydantic import BaseModel, EmailStr


class Profile(BaseModel):
    username: str
    useremail: EmailStr
    fullname: str
    blue_code: str
    gpg_key: str
    git_sign_format: str


class GitRepo(BaseModel):
    url: str
    prefix: Optional[str] = None
    dest: Optional[str] = None


class SSHConfig(BaseModel):
    key: str


class GPGConfig(BaseModel):
    key: str


class WinUser(BaseModel):
    home: str
    user: str


class KnownHost(BaseModel):
    name: str


class Config(BaseModel):
    profiles: dict[str, Profile]
    default_user_profile: str
    git_repos: list[GitRepo]
    ssh: SSHConfig
    gpg: GPGConfig
    win_user: dict[str, WinUser]
    user_apt_packages: dict[str, list[str]]
    known_hosts: list[KnownHost]

    # Optional fields that may be present
    secret_files: Optional[list[str]] = None
    github_secrets: Optional[list[dict]] = None


def load_config(file_path: Union[str, Path]) -> Config:
    """
    Load and validate configuration from YAML file.

    Args:
        file_path: Path to the YAML configuration file

    Returns:
        Validated Config object

    Raises:
        ValidationError: If the configuration doesn't match the schema
        FileNotFoundError: If the file doesn't exist
        yaml.YAMLError: If the YAML is malformed
    """
    with open(file_path) as f:
        data = yaml.safe_load(f)

    return Config(**data)


# Configuration
CONFIG_HOME = Path.home() / ".config" / "blueprint"

blueprint_config = load_config(CONFIG_HOME / "config.yaml")
