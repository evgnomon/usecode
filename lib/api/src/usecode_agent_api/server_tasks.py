"""`create_server` / `delete_server` tasks: the resumable workflows behind
POST /servers and DELETE /servers/{id}.

Neither creating nor deleting a server is a single request/response round
trip — providers take a while to actually provision or tear down a
machine, and we don't want to hold an HTTP request open for that. So the
routes only *start* the workflow and return a task id; a periodic sweep
(see app.py's lifespan) resumes it until the provider confirms the
outcome. See AGENTS.md ("Provider resources and tasks").

For creation in particular: the provider is the only source of truth for
a server's fixed attributes (IP addresses, final status, etc), and those
aren't known until the provider has actually finished provisioning. So no
`servers` row is created up front — the `requested` step just issues the
provider's create call and remembers the provider-side id; the
`confirming` step polls until the provider reports the server as reachable
(it has a public IPv4 address), and only then writes the local `servers`
row, matching the pattern used to remove it on delete.
"""

from __future__ import annotations

from pydantic import BaseModel

from .config import get_settings
from .models import CloudServerCreateIn
from .provider_credentials import decrypted_credentials
from .providers import create_server as provider_create_server
from .providers import delete_server as provider_delete_server
from .providers import list_servers as provider_list_servers
from .store import store
from .tasks import DONE, TaskContext, step

REQUESTED = "requested"
CONFIRMING = "confirming"


class _CreateRequestedPayload(BaseModel):
    provider: str
    type: str
    spec: CloudServerCreateIn


class _CreateConfirmingPayload(_CreateRequestedPayload):
    provider_server_id: str


class _DeletePayload(BaseModel):
    provider: str
    provider_server_id: str


@step("create_server", REQUESTED)
async def _create_requested(ctx: TaskContext):
    payload = _CreateRequestedPayload.model_validate(ctx.payload)
    settings = get_settings()
    credentials = await decrypted_credentials(
        settings, ctx.user_partition_key, ctx.user_id, payload.provider
    )
    if credentials is None:
        raise RuntimeError(f"No {payload.provider} credentials configured")

    created = await provider_create_server(payload.provider, credentials, payload.spec)
    next_payload = _CreateConfirmingPayload(
        **payload.model_dump(), provider_server_id=created.id
    )
    return CONFIRMING, next_payload.model_dump()


@step("create_server", CONFIRMING)
async def _create_confirming(ctx: TaskContext):
    payload = _CreateConfirmingPayload.model_validate(ctx.payload)
    settings = get_settings()
    credentials = await decrypted_credentials(
        settings, ctx.user_partition_key, ctx.user_id, payload.provider
    )
    if credentials is None:
        raise RuntimeError(f"No {payload.provider} credentials configured")

    servers = await provider_list_servers(payload.provider, credentials)
    match = next(
        (server for server in servers if server.id == payload.provider_server_id), None
    )
    if match is None or match.public_ip4 is None:
        # Still provisioning — the provider hasn't assigned a public
        # address yet. Stay parked here, the next sweep will check again.
        return CONFIRMING, ctx.payload

    await store.create_server(
        ctx.user_partition_key,
        user_id=ctx.user_id,
        provider=payload.provider,
        provider_server_id=match.id,
        type=payload.type,
        name=match.name,
        status=match.status,
        public_ip4=match.public_ip4,
        public_ip6=match.public_ip6,
    )
    return DONE, None


@step("delete_server", REQUESTED)
async def _requested(ctx: TaskContext):
    payload = _DeletePayload.model_validate(ctx.payload)
    settings = get_settings()
    credentials = await decrypted_credentials(
        settings, ctx.user_partition_key, ctx.user_id, payload.provider
    )
    if credentials is None:
        raise RuntimeError(f"No {payload.provider} credentials configured")

    await provider_delete_server(payload.provider, credentials, payload.provider_server_id)
    return CONFIRMING, ctx.payload


@step("delete_server", CONFIRMING)
async def _confirming(ctx: TaskContext):
    payload = _DeletePayload.model_validate(ctx.payload)
    settings = get_settings()
    credentials = await decrypted_credentials(
        settings, ctx.user_partition_key, ctx.user_id, payload.provider
    )
    if credentials is None:
        raise RuntimeError(f"No {payload.provider} credentials configured")

    still_there = any(
        server.id == payload.provider_server_id
        for server in await provider_list_servers(payload.provider, credentials)
    )
    if still_there:
        # Provider hasn't finished tearing the machine down yet — stay
        # parked here, the next sweep will check again.
        return CONFIRMING, ctx.payload

    for resource in ctx.resources:
        if resource["type"] == "server":
            await store.delete_server(ctx.user_partition_key, ctx.user_id, resource["id"])
    return DONE, None
