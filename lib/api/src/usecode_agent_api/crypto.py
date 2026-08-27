"""Symmetric encryption for per-user secrets stored at rest (e.g. provider credentials)."""

import base64
import hashlib
import json
from functools import lru_cache

from cryptography.fernet import Fernet

from .config import Settings


def _derive_key(secret_key: str) -> bytes:
    digest = hashlib.sha256(secret_key.encode()).digest()
    return base64.urlsafe_b64encode(digest)


@lru_cache
def _fernet(secret_key: str) -> Fernet:
    return Fernet(_derive_key(secret_key))


def encrypt(settings: Settings, value: str) -> str:
    return _fernet(settings.secret_key).encrypt(value.encode()).decode()


def decrypt(settings: Settings, token: str) -> str:
    return _fernet(settings.secret_key).decrypt(token.encode()).decode()


def encrypt_json(settings: Settings, value: dict) -> str:
    return encrypt(settings, json.dumps(value))


def decrypt_json(settings: Settings, token: str) -> dict:
    return json.loads(decrypt(settings, token))
