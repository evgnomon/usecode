"""Cloud provider registry.

Each provider module exposes:
- `validate_credentials(credentials: dict) -> None`, raising
  InvalidCredentialsError if the shape is wrong.
- an async `list_servers(credentials: dict) -> list[CloudServerOut]`.
- an async `list_server_types(credentials: dict) -> list[dict]`, each dict
  shaped `{"provider_server_type": str, "cpu": int, "memory_gb": float,
  "disk_gb": float}`.
- an async `list_locations(credentials: dict) -> list[dict]`, each dict
  including at least `{"code": str, "provider_location_code": str}` plus
  whatever else the provider returns for that location.
- an async `list_images(credentials: dict) -> list[dict]`, each dict
  including at least `{"code": str}` plus whatever else the provider
  returns for that image.
- an async `create_server(credentials: dict, spec: CloudServerCreateIn) ->
  CloudServerOut`.

`credentials` is the provider-specific JSON blob a user stored (e.g.
{"apiKey": "..."} or {"clientId": "...", "clientSecret": "..."}). Add a new
provider by dropping a module here and registering it below.
"""

from ..models import CloudServerCreateIn, CloudServerOut


class ProviderError(RuntimeError):
    def __init__(self, provider: str, status_code: int, detail: str) -> None:
        super().__init__(f"{provider} API request failed ({status_code}): {detail}")
        self.provider = provider
        self.status_code = status_code
        self.detail = detail


class InvalidCredentialsError(ValueError):
    def __init__(self, provider: str, detail: str) -> None:
        super().__init__(f"Invalid credentials for {provider}: {detail}")
        self.provider = provider
        self.detail = detail


class UnknownProviderError(ValueError):
    def __init__(self, provider: str) -> None:
        super().__init__(
            f"Unknown provider '{provider}', expected one of {sorted(PROVIDERS)}"
        )


def require_field(provider: str, credentials: dict, field: str) -> str:
    value = credentials.get(field)
    if not value:
        raise InvalidCredentialsError(provider, f"missing '{field}'")
    return value


from . import digitalocean, hetzner  # noqa: E402  (avoid circular import with the errors/helper above)

PROVIDERS = {
    "hetzner": hetzner,
    "digitalocean": digitalocean,
}


def get_provider(provider: str):
    module = PROVIDERS.get(provider)
    if module is None:
        raise UnknownProviderError(provider)
    return module


def validate_credentials(provider: str, credentials: dict) -> None:
    get_provider(provider).validate_credentials(credentials)


async def list_servers(provider: str, credentials: dict) -> list[CloudServerOut]:
    return await get_provider(provider).list_servers(credentials)


async def list_server_types(provider: str, credentials: dict) -> list[dict]:
    return await get_provider(provider).list_server_types(credentials)


async def list_locations(provider: str, credentials: dict) -> list[dict]:
    return await get_provider(provider).list_locations(credentials)


async def list_images(provider: str, credentials: dict) -> list[dict]:
    return await get_provider(provider).list_images(credentials)


async def create_server(
    provider: str, credentials: dict, spec: CloudServerCreateIn
) -> CloudServerOut:
    return await get_provider(provider).create_server(credentials, spec)


async def delete_server(provider: str, credentials: dict, server_id: str) -> None:
    await get_provider(provider).delete_server(credentials, server_id)
