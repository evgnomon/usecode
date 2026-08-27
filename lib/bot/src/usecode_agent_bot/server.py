from mcp.server.fastmcp import FastMCP

from . import compose
from .client import UsecodeAgentApiError, UsecodeAgentClient
from .compose import ComposeError
from .config import get_settings

mcp = FastMCP("usecode agent")


def _client() -> UsecodeAgentClient:
    return UsecodeAgentClient(get_settings())


@mcp.tool()
async def usecode_agent_request_otp(phone: str) -> dict:
    """Request a one-time login code for a usecode agent phone number (E.164 format, e.g. +14155552671)."""
    try:
        return await _client().request_otp(phone)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_verify_otp(phone: str, code: str) -> dict:
    """Verify a usecode agent OTP code and return an api_key for authenticated calls."""
    try:
        return await _client().verify_otp(phone, code)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_me(api_key: str | None = None) -> dict:
    """Get the phone number tied to a usecode agent api_key. Falls back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().me(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_logout(api_key: str | None = None) -> dict:
    """Revoke a usecode agent api_key, logging that client out."""
    try:
        await _client().logout(api_key)
        return {"status": "logged out"}
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_create_api_key(label: str = "", api_key: str | None = None) -> dict:
    """Generate a new usecode agent API key for the caller's account, authenticated
    with an existing api_key (or the configured USECODE_AGENT_BOT_API_KEY). Use
    `label` to note what the key is for (e.g. "laptop", "ci")."""
    try:
        return await _client().create_api_key(label, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_list_api_keys(api_key: str | None = None) -> dict:
    """List the caller's usecode agent API keys (id, label, timestamps — never the
    key value itself, which is only shown once at creation)."""
    try:
        return await _client().list_api_keys(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_revoke_api_key(key_id: str, api_key: str | None = None) -> dict:
    """Revoke one of the caller's usecode agent API keys by id."""
    try:
        await _client().revoke_api_key(key_id, api_key)
        return {"status": "revoked"}
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_health() -> dict:
    """Check whether usecode agent is reachable. Reports every configured endpoint
    (the Caddy load balancers requests are spread over round-robin), not
    just the one the next request would land on, so a single dead load
    balancer shows up instead of being silently failed over. Each reachable
    entry names the API node that answered it."""
    try:
        endpoints = await _client().health_all()
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}
    reachable = [entry for entry in endpoints if entry.get("reachable")]
    return {
        "status": "ok" if reachable else "unreachable",
        "reachable": len(reachable),
        "endpoints": endpoints,
    }


@mcp.tool()
async def usecode_agent_model_options(api_key: str | None = None) -> dict:
    """List the configurable fields for kick-starting the AI model container
    (llama-server), each with its default value and, where applicable, its
    allowed options. Falls back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().model_options(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_model_status(api_key: str | None = None) -> dict:
    """Check whether the AI model container (llama-server) is currently running
    on the usecode-agent-api host. Falls back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().model_status(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_model_start(
    image: str | None = None,
    hf_repo: str | None = None,
    device: str | None = None,
    ngl: int | None = None,
    alias: str | None = None,
    ctx_size: int | None = None,
    host: str | None = None,
    port: int | None = None,
    api_key: str | None = None,
) -> dict:
    """Kick-start the AI model container (llama-server) on the usecode-agent-api host.
    Defaults to `ggml-org/Qwen3-0.6B-GGUF:Q4_0` on device `Vulkan0` with a
    32768-token context, alias `local-model`, on `127.0.0.1:8080` — call
    usecode_agent_model_options for the full default/options list. Omit any field to
    keep its default; pass a value to override just that field. Falls back to
    the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().model_start(
            api_key=api_key,
            image=image,
            hf_repo=hf_repo,
            device=device,
            ngl=ngl,
            alias=alias,
            ctx_size=ctx_size,
            host=host,
            port=port,
        )
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_model_stop(api_key: str | None = None) -> dict:
    """Stop the running AI model container (llama-server) on the usecode-agent-api host.
    Falls back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        await _client().model_stop(api_key)
        return {"status": "stopped"}
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_set_provider_credentials(
    provider: str, credentials: dict, api_key: str | None = None
) -> dict:
    """Store the caller's credentials for a cloud provider, encrypted at
    rest, so servers of that provider's type series can be created/synced.
    `credentials` is a JSON object whose shape depends on `provider`:
    - "hetzner": {"apiKey": "<hetzner cloud api token>"}
    - "digitalocean": {"apiKey": "<digitalocean api token>"}
    Other providers may require different fields (e.g. clientId/
    clientSecret) — check that provider's docs. Falls back to the
    configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().set_provider_credentials(provider, credentials, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_provider_credentials_status(
    provider: str, api_key: str | None = None
) -> dict:
    """Check whether the caller has credentials configured for one cloud
    provider ("hetzner" or "digitalocean"). Falls back to the configured
    USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().provider_credentials_status(provider, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_delete_provider_credentials(
    provider: str, api_key: str | None = None
) -> dict:
    """Remove the caller's stored credentials for a cloud provider
    ("hetzner" or "digitalocean"). Falls back to the configured
    USECODE_AGENT_BOT_API_KEY."""
    try:
        await _client().delete_provider_credentials(provider, api_key)
        return {"status": "deleted"}
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_list_provider_credentials(api_key: str | None = None) -> dict:
    """List every supported cloud provider ("hetzner", "digitalocean") and
    whether the caller has credentials configured for it. Falls back to the
    configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().list_provider_credentials(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_list_servers(api_key: str | None = None) -> dict:
    """List the caller's servers, in usecode agent's own terms — id, name, type
    (e.g. "x1-fsn1", "y2-nyc3"), status, public IPs. Which cloud provider
    actually hosts a server is an internal detail, not exposed here. Falls
    back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().list_servers(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_list_server_types(api_key: str | None = None) -> dict:
    """List every server type available across the caller's configured
    provider credentials — usecode agent's own series (e.g. "x1", "y2"; no city,
    since specs don't vary by city) with cpu, memory, and main-disk specs.
    Calling this also mints a stable series for any provider type not seen
    before, so it can be passed to usecode_agent_create_server afterwards. Falls
    back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().list_server_types(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_get_server(server_id: str, api_key: str | None = None) -> dict:
    """Get one of the caller's servers by its usecode agent server id. Falls back
    to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().get_server(server_id, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_create_server(
    name: str,
    type: str,
    image: str = "ubuntu-24.04",
    ssh_keys: list[str] | None = None,
    api_key: str | None = None,
) -> dict:
    """Create a new server. `type` is usecode agent's own series-city type string,
    not a cloud-provider type — e.g. "x1"/"x2"/"x4"/"x8" plus a city, such
    as "x1-fsn1" or "x8-ash"; or "y1"/"y2"/"y4"/"y8" plus a city, such as
    "y1-nyc3". `image` is the OS image slug (e.g. "ubuntu-24.04"). Run
    usecode_agent_sync_servers then usecode_agent_list_catalog (kind="location" or
    kind="image") to see the caller's actual valid values instead of
    guessing. Requires credentials configured for whichever provider that
    type maps to, via usecode_agent_set_provider_credentials. Provisioning runs as a background
    task (the server's IP and final status aren't known until the provider
    finishes), so this schedules the task and returns it; poll it with
    usecode_agent_get_task until it 404s (meaning it finished), then use
    usecode_agent_list_servers to find the new server. Falls back to the
    configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().create_server(
            name=name, type=type, image=image, ssh_keys=ssh_keys, api_key=api_key
        )
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_delete_server(server_id: str, api_key: str | None = None) -> dict:
    """Delete a server by its usecode agent server id. This is irreversible — the
    server and its data are destroyed. Deletion runs as a background task
    (the provider can take a while to tear the machine down), so this
    schedules the task and returns it; poll it with usecode_agent_get_task until
    its state stops changing and it 404s (meaning it finished and the
    server is gone). Falls back to the configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().delete_server(server_id, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_list_tasks(api_key: str | None = None) -> dict:
    """List the caller's in-flight background tasks (create_server/delete_server
    workflows started by usecode_agent_create_server/usecode_agent_delete_server that
    haven't finished yet — a task disappears from this list once it's done,
    same as when usecode_agent_get_task starts 404ing for it). Falls back to the
    configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().list_tasks(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_get_task(task_id: str, api_key: str | None = None) -> dict:
    """Get the status of a background task (e.g. one started by
    usecode_agent_delete_server) by its id. A 404-shaped error response means the
    task finished and was cleaned up. Falls back to the configured
    USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().get_task(task_id, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_sync_servers(api_key: str | None = None) -> dict:
    """Fetch every server already provisioned with the caller's configured
    provider credentials and make sure each one is reflected in usecode agent's
    database (matched by the provider's own server id), so newly-created or
    externally-created servers show up in usecode_agent_list_servers. Also
    mirrors each configured provider's full catalog (locations, server
    types, OS images) into usecode agent's database, and fixes the series/city
    mappings used by "x1-fsn1"/"y1-nyc3"-style type strings — see
    usecode_agent_list_catalog to inspect what was stored. Falls back to the
    configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().sync_servers(api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
async def usecode_agent_list_catalog(
    provider: str | None = None, kind: str | None = None, api_key: str | None = None
) -> dict:
    """List the provider catalog data mirrored by the most recent
    usecode_agent_sync_servers call — every location, server type, and OS image
    each configured provider offers, as raw provider data. Optionally
    filter by `provider` ("hetzner"/"digitalocean") and/or `kind`
    ("location"/"server_type"/"image"). Use this to see valid city codes
    (e.g. what to put after the "-" in "x1-fsn1") and valid `image` values
    for usecode_agent_create_server, instead of guessing at provider naming. Run
    usecode_agent_sync_servers first if this comes back empty. Falls back to the
    configured USECODE_AGENT_BOT_API_KEY."""
    try:
        return await _client().list_catalog(provider, kind, api_key)
    except UsecodeAgentApiError as exc:
        return {"error": exc.detail, "status_code": exc.status_code}


@mcp.tool()
def usecode_agent_ensure_running() -> dict:
    """Make sure usecode agent is running on this machine, starting it via deploy/compose.yml if not."""
    settings = get_settings()
    try:
        if compose.is_running(settings):
            return {"status": "already running"}
        result = compose.start(settings)
        return {"status": "started", **result}
    except ComposeError as exc:
        return {"error": str(exc)}


@mcp.tool()
def usecode_agent_stop() -> dict:
    """Stop usecode agent on this machine by tearing down the deploy/compose.yml stack."""
    settings = get_settings()
    try:
        result = compose.stop(settings)
        return {"status": "stopped", **result}
    except ComposeError as exc:
        return {"error": str(exc)}


@mcp.tool()
def usecode_agent_bot_reload() -> dict:
    """Reload usecode agent by rebuilding and recreating the deploy/compose.yml
    stack: `compose up -d --build --force-recreate`. Use this after making
    code changes (e.g. to lib/api or lib/bot) to pick them up in the
    running containers."""
    settings = get_settings()
    try:
        result = compose.start(settings)
        return {"status": "reloaded", **result}
    except ComposeError as exc:
        return {"error": str(exc)}


@mcp.tool()
def usecode_agent_logs_commands() -> dict:
    """List shell commands to follow logs for each service in deploy/compose.yml (and all of them combined)."""
    settings = get_settings()
    try:
        return compose.logs_commands(settings)
    except ComposeError as exc:
        return {"error": str(exc)}
