"""Shared helpers for looking up a user's decrypted provider credentials.

Used by both routes/providers.py (credential management) and
routes/servers.py (server operations, which need credentials for whichever
provider a server's type maps to).
"""

from fastapi import HTTPException, status

from . import crypto
from .config import Settings
from .providers import PROVIDERS, UnknownProviderError
from .store import store


def check_known_provider(provider: str) -> None:
    if provider not in PROVIDERS:
        raise HTTPException(
            status.HTTP_404_NOT_FOUND, detail=str(UnknownProviderError(provider))
        )


async def decrypted_credentials(
    settings: Settings, partition_key: str | None, user_id: str, provider: str
) -> dict | None:
    """A user's credentials live on their own database instance, so the
    caller has to say which one — `partition_key` comes from the
    authenticated client, or from the task's partition."""
    credentials_encrypted = await store.get_provider_credentials(
        partition_key, user_id, provider
    )
    if credentials_encrypted is None:
        return None
    return crypto.decrypt_json(settings, credentials_encrypted)
