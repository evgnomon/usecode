import re
from typing import Literal

from pydantic import BaseModel, field_validator

PHONE_RE = re.compile(r"^\+?[1-9]\d{7,14}$")

ServerStatus = Literal["up", "paused"]

# Raw provider/internal server states that count as "up" — everything else
# (off, new, archive, deleting, migrating, rebuilding, unknown, ...) is
# "paused". Hetzner and DigitalOcean use disjoint vocabularies, so this maps
# both into the two states our API ever exposes.
_UP_STATUSES = {"running", "active", "starting", "initializing"}


def normalize_server_status(raw: str) -> ServerStatus:
    return "up" if raw.lower() in _UP_STATUSES else "paused"


def normalize_phone(value: str) -> str:
    value = value.strip().replace(" ", "").replace("-", "")
    if not PHONE_RE.match(value):
        raise ValueError("Phone number must be in E.164-like format, e.g. +14155552671")
    return value if value.startswith("+") else f"+{value}"


class OtpRequestIn(BaseModel):
    phone: str

    @field_validator("phone")
    @classmethod
    def validate_phone(cls, value: str) -> str:
        return normalize_phone(value)


class OtpRequestOut(BaseModel):
    phone: str
    expires_in: int
    resend_after: int
    debug_code: str | None = None


class OtpVerifyIn(BaseModel):
    phone: str
    code: str

    @field_validator("phone")
    @classmethod
    def validate_phone(cls, value: str) -> str:
        return normalize_phone(value)

    @field_validator("code")
    @classmethod
    def validate_code(cls, value: str) -> str:
        value = value.strip()
        if not value.isdigit():
            raise ValueError("Code must be numeric")
        return value


class AuthTokenOut(BaseModel):
    api_key: str
    phone: str


class MeOut(BaseModel):
    phone: str
    created_at: float


class ApiKeyCreateIn(BaseModel):
    label: str = ""


class ApiKeyCreateOut(BaseModel):
    id: str
    api_key: str
    label: str
    created_at: float


class ApiKeyOut(BaseModel):
    id: str
    label: str
    created_at: float
    last_used_at: float | None = None


class ApiKeyListOut(BaseModel):
    api_keys: list[ApiKeyOut]


class ModelFieldOut(BaseModel):
    name: str
    default: object
    options: list | None = None
    description: str = ""


class ModelOptionsOut(BaseModel):
    fields: list[ModelFieldOut]


class ModelStartIn(BaseModel):
    image: str | None = None
    hf_repo: str | None = None
    device: str | None = None
    ngl: int | None = None
    alias: str | None = None
    ctx_size: int | None = None
    host: str | None = None
    port: int | None = None


class ModelStatusOut(BaseModel):
    running: bool
    config: dict | None = None
    state: dict | None = None


class CloudServerOut(BaseModel):
    """Raw shape returned by a provider client (internal use only — never
    exposed directly over the public API, which speaks our own server
    IDs/types instead)."""

    provider: str
    id: str
    name: str
    status: str
    server_type: str
    location: str
    public_ip4: str | None = None
    public_ip6: str | None = None
    created: str


class CloudServerCreateIn(BaseModel):
    """Provider-facing create spec (internal use only)."""

    provider: str = "hetzner"
    name: str
    server_type: str
    image: str
    location: str | None = None
    ssh_keys: list[str] = []


class ServerCreateIn(BaseModel):
    # Our own type terminology, e.g. "x1-fsn1" or "y1-nyc3" — series (x1,
    # x2, x4, x8 for Hetzner; y1, y2, y4, y8 for DigitalOcean) plus the
    # provider's city/region code.
    name: str
    type: str
    image: str = "ubuntu-24.04"
    ssh_keys: list[str] = []


class ServerOut(BaseModel):
    id: str
    name: str
    type: str
    status: ServerStatus
    public_ip4: str | None = None
    public_ip6: str | None = None
    created: str


class ServerListOut(BaseModel):
    servers: list[ServerOut]


class ServerTypeOut(BaseModel):
    # Our own series identifier, e.g. "x1" or "y2" (no city — specs don't
    # vary by city, only by series).
    type: str
    cpu: int
    memory_gb: float
    disk_gb: float
    # Our own city codes this series is available in, e.g. ["fsn", "ash"] —
    # combine one with `type` (as "{type}-{city}") to build a value valid
    # for POST /servers' `type` field.
    cities: list[str] = []


class ServerTypeListOut(BaseModel):
    types: list[ServerTypeOut]


class ServerSyncOut(BaseModel):
    added: int
    updated: int
    servers: list[ServerOut]


class ProviderResourceOut(BaseModel):
    # Raw catalog data mirrored from a provider by POST /servers/sync.
    provider: str
    kind: str  # "location", "server_type", or "image"
    code: str  # the provider's own identifier for this resource
    data: dict


class ProviderResourceListOut(BaseModel):
    resources: list[ProviderResourceOut]


class TaskOut(BaseModel):
    id: str
    kind: str
    # Node name of the API instance carrying this task to completion.
    # Exposed so a caller can see which of the horizontally-scaled nodes
    # owns the work — and it is also where the task row lives, since
    # `tasks` is partitioned on it.
    assignee: str
    state: str
    resources: list[dict]
    error: str | None = None
    created_at: float
    updated_at: float


class TaskListOut(BaseModel):
    tasks: list[TaskOut]


class ProviderCredentialsIn(BaseModel):
    # Shape is provider-specific, e.g. {"apiKey": "..."} for Hetzner and
    # DigitalOcean, or {"clientId": "...", "clientSecret": "..."} for
    # providers that use OAuth-style client credentials.
    credentials: dict

    @field_validator("credentials")
    @classmethod
    def validate_credentials(cls, value: dict) -> dict:
        if not value:
            raise ValueError("credentials must not be empty")
        return value


class ProviderCredentialsStatusOut(BaseModel):
    provider: str
    configured: bool


class ProviderCredentialsListOut(BaseModel):
    providers: list[ProviderCredentialsStatusOut]
