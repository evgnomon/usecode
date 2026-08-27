"""DigitalOcean provider client."""

import re

import httpx

from ..models import CloudServerCreateIn, CloudServerOut
from . import ProviderError, require_field

API_BASE = "https://api.digitalocean.com/v2"

# DigitalOcean's own region slugs ("nyc1", "nyc2", "nyc3", "sfo2", "sfo3", ...)
# append a datacenter number to the city code — our own terminology drops it,
# picking the highest-numbered (newest) datacenter as the one canonical region
# per city, the same way Hetzner's "fsn1" collapses to "fsn".
def _base_city(slug: str) -> str:
    return re.sub(r"\d+$", "", slug)


def _public_ip(addresses: list[dict]) -> str | None:
    for address in addresses:
        if address.get("type") == "public":
            return address.get("ip_address")
    return None


def _to_server_out(droplet: dict) -> CloudServerOut:
    networks = droplet.get("networks") or {}
    return CloudServerOut(
        provider="digitalocean",
        id=str(droplet["id"]),
        name=droplet["name"],
        status=droplet["status"],
        server_type=droplet.get("size_slug", ""),
        location=_base_city((droplet.get("region") or {}).get("slug", "")),
        public_ip4=_public_ip(networks.get("v4") or []),
        public_ip6=_public_ip(networks.get("v6") or []),
        created=droplet["created_at"],
    )


def validate_credentials(credentials: dict) -> None:
    require_field("digitalocean", credentials, "apiKey")


async def list_servers(credentials: dict) -> list[CloudServerOut]:
    token = require_field("digitalocean", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/droplets")
    if response.status_code != 200:
        raise ProviderError("digitalocean", response.status_code, response.text)

    return [_to_server_out(droplet) for droplet in response.json()["droplets"]]


async def create_server(credentials: dict, spec: CloudServerCreateIn) -> CloudServerOut:
    token = require_field("digitalocean", credentials, "apiKey")
    if not spec.location:
        raise ProviderError("digitalocean", 400, "location (region) is required")

    headers = {"Authorization": f"Bearer {token}"}
    body = {
        "name": spec.name,
        "size": spec.server_type,
        "image": spec.image,
        "region": spec.location,
    }
    if spec.ssh_keys:
        body["ssh_keys"] = spec.ssh_keys

    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.post("/droplets", json=body)
    if response.status_code != 202:
        raise ProviderError("digitalocean", response.status_code, response.text)

    return _to_server_out(response.json()["droplet"])


async def list_server_types(credentials: dict) -> list[dict]:
    token = require_field("digitalocean", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/sizes")
    if response.status_code != 200:
        raise ProviderError("digitalocean", response.status_code, response.text)

    return [
        {
            "provider_server_type": size["slug"],
            "cpu": size["vcpus"],
            "memory_gb": size["memory"] / 1024,
            "disk_gb": size["disk"],
            "cities": sorted({_base_city(region) for region in size.get("regions", [])}),
        }
        for size in response.json()["sizes"]
        if size.get("available", True)
    ]


async def list_locations(credentials: dict) -> list[dict]:
    token = require_field("digitalocean", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/regions")
    if response.status_code != 200:
        raise ProviderError("digitalocean", response.status_code, response.text)

    by_city: dict[str, dict] = {}
    for region in response.json()["regions"]:
        if not region.get("available", True):
            continue
        slug = region["slug"]
        city = _base_city(slug)
        existing = by_city.get(city)
        if existing is None or slug > existing["provider_location_code"]:
            by_city[city] = {"code": city, "provider_location_code": slug, **region}
    return list(by_city.values())


async def list_images(credentials: dict) -> list[dict]:
    token = require_field("digitalocean", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/images", params={"type": "distribution", "per_page": 200})
    if response.status_code != 200:
        raise ProviderError("digitalocean", response.status_code, response.text)

    return [{"code": image["slug"], **image} for image in response.json()["images"] if image.get("slug")]


async def delete_server(credentials: dict, server_id: str) -> None:
    token = require_field("digitalocean", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.delete(f"/droplets/{server_id}")
    if response.status_code != 204:
        raise ProviderError("digitalocean", response.status_code, response.text)
