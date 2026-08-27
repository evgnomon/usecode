import asyncio
from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException, status

from ..config import Settings, get_settings
from ..models import (
    CloudServerCreateIn,
    ProviderResourceListOut,
    ProviderResourceOut,
    ServerCreateIn,
    ServerListOut,
    ServerOut,
    ServerSyncOut,
    ServerTypeListOut,
    ServerTypeOut,
    TaskOut,
    normalize_server_status,
)
from ..provider_credentials import decrypted_credentials
from ..providers import InvalidCredentialsError, ProviderError
from ..providers import list_images as provider_list_images
from ..providers import list_locations as provider_list_locations
from ..providers import list_server_types as provider_list_server_types
from ..providers import list_servers as provider_list_servers
from .. import server_tasks  # noqa: F401  (registers the create_server/delete_server task steps)
from ..security import get_current_client
from ..store import ApiKeyRecord, ServerRecord, TaskRecord, store
from ..tasks import advance as advance_task
from ..tasks import create_task

router = APIRouter(prefix="/servers", tags=["servers"], dependencies=[Depends(get_current_client)])


def _to_out(server: ServerRecord) -> ServerOut:
    return ServerOut(
        id=server.id,
        name=server.name,
        type=server.type,
        status=normalize_server_status(server.status),
        public_ip4=server.public_ip4,
        public_ip6=server.public_ip6,
        created=datetime.fromtimestamp(server.created_at, tz=timezone.utc).isoformat(),
    )


def _task_to_out(task: TaskRecord) -> TaskOut:
    return TaskOut(
        id=task.id,
        kind=task.kind,
        assignee=task.assignee,
        state=task.state,
        resources=task.resources,
        error=task.error,
        created_at=task.created_at,
        updated_at=task.updated_at,
    )


async def _resolve_type(type_: str) -> tuple[str, str, str]:
    """Split "{series}-{city}" and look up the series' provider + provider
    server type, and the city's provider location code (via the fixed
    mapping POST /servers/sync builds). Returns (provider,
    provider_server_type, provider_location_code). Both the series and the
    city must already be known (via a prior /servers/types or /servers/sync
    call) — neither is accepted unresolved, so e.g. a provider's own raw
    region slug like 'nyc3' is rejected in favor of our own city code 'nyc'."""
    series, sep, city = type_.partition("-")
    if not sep or not city:
        raise HTTPException(
            status.HTTP_400_BAD_REQUEST,
            detail=f"Invalid server type '{type_}', expected '<series>-<city>' e.g. 'x1-fsn'",
        )
    mapping = await store.get_server_type_mapping(series)
    if mapping is None:
        raise HTTPException(
            status.HTTP_400_BAD_REQUEST, detail=f"Unknown server type series '{series}'"
        )
    location = await store.get_location_mapping(city, mapping.provider)
    if location is None:
        raise HTTPException(
            status.HTTP_400_BAD_REQUEST,
            detail=f"City '{city}' is not available for series '{series}'",
        )
    return mapping.provider, mapping.provider_server_type, location.provider_location_code


@router.post("", response_model=TaskOut, status_code=status.HTTP_202_ACCEPTED)
async def create_server(
    payload: ServerCreateIn,
    client: ApiKeyRecord = Depends(get_current_client),
    settings: Settings = Depends(get_settings),
) -> TaskOut:
    """Start the create_server task. Provisioning a server is a
    provider-side workflow that can take a while, and the server's fixed
    attributes (IP addresses, final status) aren't known until the
    provider finishes — so no `servers` row exists until then either. This
    only schedules the workflow; poll GET /tasks/{task_id} until it
    404s (meaning it finished), then GET /servers to find the new server."""
    provider, provider_server_type, provider_location_code = await _resolve_type(payload.type)
    credentials = await decrypted_credentials(
        settings, client.partition_key, client.user_id, provider
    )
    if credentials is None:
        raise HTTPException(
            status.HTTP_400_BAD_REQUEST,
            detail=f"No {provider} credentials configured, set some via PUT /providers/{provider}/credentials",
        )

    spec = CloudServerCreateIn(
        provider=provider,
        name=payload.name,
        server_type=provider_server_type,
        image=payload.image,
        location=provider_location_code,
        ssh_keys=payload.ssh_keys,
    )
    task = await create_task(
        user_id=client.user_id,
        kind="create_server",
        initial_state="requested",
        resources=[],
        payload={"provider": provider, "type": payload.type, "spec": spec.model_dump()},
    )
    try:
        advanced = await advance_task(task.assignee, task.id)
    except InvalidCredentialsError as exc:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except ProviderError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    return _task_to_out(advanced or task)


@router.get("", response_model=ServerListOut)
async def list_servers(client: ApiKeyRecord = Depends(get_current_client)) -> ServerListOut:
    servers = await store.list_servers(client.partition_key, client.user_id)
    return ServerListOut(servers=[_to_out(server) for server in servers])


@router.get("/types", response_model=ServerTypeListOut)
async def list_server_types(
    client: ApiKeyRecord = Depends(get_current_client),
    settings: Settings = Depends(get_settings),
) -> ServerTypeListOut:
    """List every server type available across the caller's configured
    providers, keyed by our own series (e.g. "x1", "y2" — no city, since
    specs don't vary by city) with cpu, memory, main-disk specs, and the
    city codes it's available in. Combine a `type` with one of its `cities`
    (as "{type}-{city}") to get a value valid for POST /servers. Also mints
    a stable series for any provider type not seen before, and records any
    newly-seen cities against it, so it can be passed to POST /servers
    afterwards."""
    configured = await store.list_configured_providers(
        client.partition_key, client.user_id
    )

    async def _fetch(provider: str) -> list[dict]:
        credentials = await decrypted_credentials(
            settings, client.partition_key, client.user_id, provider
        )
        assert credentials is not None
        try:
            raw_types = await provider_list_server_types(provider, credentials)
        except InvalidCredentialsError as exc:
            raise HTTPException(status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
        except ProviderError as exc:
            raise HTTPException(status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
        return [{"provider": provider, **raw_type} for raw_type in raw_types]

    results = await asyncio.gather(*(_fetch(provider) for provider in configured))

    types: list[ServerTypeOut] = []
    for raw_types in results:
        for raw_type in raw_types:
            series = await store.get_or_create_series_for_provider_type(
                raw_type["provider"],
                raw_type["provider_server_type"],
                cities=raw_type.get("cities"),
            )
            cities = await store.get_server_type_cities(series)
            types.append(
                ServerTypeOut(
                    type=series,
                    cpu=raw_type["cpu"],
                    memory_gb=raw_type["memory_gb"],
                    disk_gb=raw_type["disk_gb"],
                    cities=cities,
                )
            )

    types.sort(key=lambda t: (t.type[:1], int(t.type[1:]) if t.type[1:].isdigit() else 0))
    return ServerTypeListOut(types=types)


@router.get("/catalog", response_model=ProviderResourceListOut)
async def list_catalog(
    provider: str | None = None,
    kind: str | None = None,
    client: ApiKeyRecord = Depends(get_current_client),
) -> ProviderResourceListOut:
    """List the provider catalog data mirrored by the most recent
    POST /servers/sync — locations, server types, and OS images, optionally
    filtered by `provider` ("hetzner"/"digitalocean") and/or `kind`
    ("location"/"server_type"/"image"). This is how to see every value
    valid for building a server type string or picking an image, instead of
    guessing at provider naming."""
    resources = await store.list_provider_resources(provider=provider, kind=kind)
    return ProviderResourceListOut(
        resources=[
            ProviderResourceOut(
                provider=resource.provider,
                kind=resource.kind,
                code=resource.code,
                data=resource.data,
            )
            for resource in resources
        ]
    )


@router.get("/{server_id}", response_model=ServerOut)
async def get_server(
    server_id: str, client: ApiKeyRecord = Depends(get_current_client)
) -> ServerOut:
    server = await store.get_server(client.partition_key, client.user_id, server_id)
    if server is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, detail="Server not found")
    return _to_out(server)


@router.delete("/{server_id}", response_model=TaskOut, status_code=status.HTTP_202_ACCEPTED)
async def delete_server(
    server_id: str,
    client: ApiKeyRecord = Depends(get_current_client),
    settings: Settings = Depends(get_settings),
) -> TaskOut:
    """Start the delete_server task. Deleting a server is a provider-side
    workflow that can take a while, so this only schedules it — poll
    GET /tasks/{task_id} (or GET /servers/{server_id}, which
    disappears once the task completes) to see it finish."""
    server = await store.get_server(client.partition_key, client.user_id, server_id)
    if server is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, detail="Server not found")

    credentials = await decrypted_credentials(
        settings, client.partition_key, client.user_id, server.provider
    )
    if credentials is None:
        raise HTTPException(
            status.HTTP_400_BAD_REQUEST,
            detail=f"No {server.provider} credentials configured",
        )

    await store.set_server_status(
        client.partition_key, client.user_id, server_id, "deleting"
    )
    task = await create_task(
        user_id=client.user_id,
        kind="delete_server",
        initial_state="requested",
        resources=[{"type": "server", "id": server_id}],
        payload={"provider": server.provider, "provider_server_id": server.provider_server_id},
    )
    try:
        advanced = await advance_task(task.assignee, task.id)
    except InvalidCredentialsError as exc:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except ProviderError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    return _task_to_out(advanced or task)


async def _sync_catalog(provider: str, credentials: dict) -> None:
    """Mirror everything needed to fill out a create-server spec for one
    provider into our db: every location, server type, and OS image it
    offers (raw, as provider_resources rows), plus the fixed series/city
    mappings our own type strings are built from."""
    locations, server_types, images = await asyncio.gather(
        provider_list_locations(provider, credentials),
        provider_list_server_types(provider, credentials),
        provider_list_images(provider, credentials),
    )

    for location in locations:
        await store.upsert_provider_resource(provider, "location", location["code"], location)
        await store.set_location_mapping(
            location["code"], provider, location["provider_location_code"]
        )

    for server_type in server_types:
        await store.upsert_provider_resource(
            provider, "server_type", server_type["provider_server_type"], server_type
        )
        await store.get_or_create_series_for_provider_type(
            provider, server_type["provider_server_type"], cities=server_type.get("cities")
        )

    for image in images:
        await store.upsert_provider_resource(provider, "image", image["code"], image)


@router.post("/sync", response_model=ServerSyncOut)
async def sync_servers(
    client: ApiKeyRecord = Depends(get_current_client),
    settings: Settings = Depends(get_settings),
) -> ServerSyncOut:
    """Fetch every server already provisioned with the caller's configured
    provider credentials and make sure each one is reflected in our
    database, matched by the provider's own server id. Also mirrors each
    provider's full catalog (locations, server types, OS images) into our
    db, and fixes the series/city mappings our own type strings use — see
    GET /servers/catalog to inspect what was stored."""
    configured = await store.list_configured_providers(
        client.partition_key, client.user_id
    )

    async def _credentials(provider: str) -> dict:
        credentials = await decrypted_credentials(
            settings, client.partition_key, client.user_id, provider
        )
        assert credentials is not None
        return credentials

    async def _fetch(provider: str) -> list:
        credentials = await _credentials(provider)
        try:
            servers = await provider_list_servers(provider, credentials)
            await _sync_catalog(provider, credentials)
            return servers
        except InvalidCredentialsError as exc:
            raise HTTPException(status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
        except ProviderError as exc:
            raise HTTPException(status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc

    results = await asyncio.gather(*(_fetch(provider) for provider in configured))

    added = 0
    updated = 0
    synced: list[ServerOut] = []
    for provider_servers in results:
        for provider_server in provider_servers:
            series = await store.get_or_create_series_for_provider_type(
                provider_server.provider, provider_server.server_type
            )
            await store.set_location_mapping(
                provider_server.location, provider_server.provider, provider_server.location
            )
            type_ = f"{series}-{provider_server.location}"
            record, created = await store.upsert_server_by_provider_id(
                client.partition_key,
                user_id=client.user_id,
                provider=provider_server.provider,
                provider_server_id=provider_server.id,
                type=type_,
                name=provider_server.name,
                status=provider_server.status,
                public_ip4=provider_server.public_ip4,
                public_ip6=provider_server.public_ip6,
            )
            added += int(created)
            updated += int(not created)
            synced.append(_to_out(record))

    return ServerSyncOut(added=added, updated=updated, servers=synced)
