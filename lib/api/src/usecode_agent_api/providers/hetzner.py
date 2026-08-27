"""Hetzner Cloud provider client."""

import httpx

from ..models import CloudServerCreateIn, CloudServerOut
from . import ProviderError, require_field

API_BASE = "https://api.hetzner.cloud/v1"

# Hetzner's own location codes ("fsn1", "nbg1", "hel1") append a trailing
# datacenter number to the city code — our own terminology drops it, since
# each city currently maps to exactly one Hetzner location.
_LOCATIONS = {"ash": "ash", "fsn": "fsn1", "hel": "hel1", "hil": "hil", "nbg": "nbg1", "sin": "sin"}
_CITIES = {location: city for city, location in _LOCATIONS.items()}


def _city(location: str) -> str:
    return _CITIES.get(location, location)


def _location(city: str) -> str:
    return _LOCATIONS.get(city, city)


def _to_server_out(server: dict) -> CloudServerOut:
    public_net = server.get("public_net") or {}
    ipv4 = public_net.get("ipv4") or {}
    ipv6 = public_net.get("ipv6") or {}
    return CloudServerOut(
        provider="hetzner",
        id=str(server["id"]),
        name=server["name"],
        status=server["status"],
        server_type=(server.get("server_type") or {}).get("name", ""),
        location=_city((server.get("location") or {}).get("name", "")),
        public_ip4=ipv4.get("ip"),
        public_ip6=ipv6.get("ip"),
        created=server["created"],
    )


def validate_credentials(credentials: dict) -> None:
    require_field("hetzner", credentials, "apiKey")


async def list_servers(credentials: dict) -> list[CloudServerOut]:
    token = require_field("hetzner", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/servers")
    if response.status_code != 200:
        raise ProviderError("hetzner", response.status_code, response.text)

    return [_to_server_out(server) for server in response.json()["servers"]]


async def create_server(credentials: dict, spec: CloudServerCreateIn) -> CloudServerOut:
    token = require_field("hetzner", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    body = {
        "name": spec.name,
        "server_type": spec.server_type,
        "image": spec.image,
    }
    if spec.location:
        body["location"] = _location(spec.location)
    if spec.ssh_keys:
        body["ssh_keys"] = spec.ssh_keys

    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.post("/servers", json=body)
    if response.status_code != 201:
        raise ProviderError("hetzner", response.status_code, response.text)

    return _to_server_out(response.json()["server"])


async def list_server_types(credentials: dict) -> list[dict]:
    token = require_field("hetzner", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/server_types")
    if response.status_code != 200:
        raise ProviderError("hetzner", response.status_code, response.text)

    return [
        {
            "provider_server_type": server_type["name"],
            "cpu": server_type["cores"],
            "memory_gb": server_type["memory"],
            "disk_gb": server_type["disk"],
            "cities": sorted(
                {_city(price["location"]) for price in server_type.get("prices", [])}
            ),
        }
        for server_type in response.json()["server_types"]
        if not server_type.get("deprecated")
    ]


async def list_locations(credentials: dict) -> list[dict]:
    token = require_field("hetzner", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/locations")
    if response.status_code != 200:
        raise ProviderError("hetzner", response.status_code, response.text)

    return [
        {"code": _city(location["name"]), "provider_location_code": location["name"], **location}
        for location in response.json()["locations"]
    ]


async def list_images(credentials: dict) -> list[dict]:
    token = require_field("hetzner", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.get("/images", params={"type": "system"})
    if response.status_code != 200:
        raise ProviderError("hetzner", response.status_code, response.text)

    return [{"code": image["name"], **image} for image in response.json()["images"]]


async def delete_server(credentials: dict, server_id: str) -> None:
    token = require_field("hetzner", credentials, "apiKey")
    headers = {"Authorization": f"Bearer {token}"}
    async with httpx.AsyncClient(base_url=API_BASE, headers=headers, timeout=10.0) as client:
        response = await client.delete(f"/servers/{server_id}")
    if response.status_code != 200:
        raise ProviderError("hetzner", response.status_code, response.text)
