import itertools

import httpx

from .config import Settings

# Failures that prove the request never reached an API instance: the
# connection was refused, or timed out before it was established. Retrying
# these against another load balancer is always safe, whatever the method.
_UNREACHABLE = (httpx.ConnectError, httpx.ConnectTimeout)

# Statuses Caddy returns when *it* is up but had no healthy API instance to
# forward to. The request may or may not have been processed, so these are
# only retried for methods that can be repeated without doubling an effect.
_GATEWAY_STATUSES = frozenset({502, 503, 504})
_REPEATABLE_METHODS = frozenset({"GET", "HEAD", "OPTIONS", "PUT", "DELETE"})

# Round-robin cursor. Shared across clients on purpose: server.py builds a
# fresh UsecodeAgentClient for every tool call, so a per-instance cursor would
# start at the same endpoint every time and never rotate.
_cursor = itertools.count()


class UsecodeAgentApiError(RuntimeError):
    def __init__(self, status_code: int, detail: str) -> None:
        super().__init__(f"usecode agent API error {status_code}: {detail}")
        self.status_code = status_code
        self.detail = detail


class UsecodeAgentUnreachableError(UsecodeAgentApiError):
    """No configured endpoint could be reached at all. A subclass of
    UsecodeAgentApiError (reported as 503) so every tool's existing error
    handling covers it — from a caller's point of view "every load balancer
    is down" is just another unavailable answer."""

    def __init__(self, endpoints: list[str], last_error: Exception | None) -> None:
        joined = ", ".join(endpoints) or "<none configured>"
        super().__init__(503, f"No usecode agent endpoint reachable ({joined}): {last_error}")
        self.endpoints = endpoints
        self.last_error = last_error


class UsecodeAgentClient:
    """Thin async wrapper around the usecode-agent-api HTTP endpoints, spread over
    the configured Caddy load balancers (see Settings.endpoints)."""

    def __init__(self, settings: Settings) -> None:
        self._settings = settings

    def _headers(self, api_key: str | None) -> dict[str, str]:
        key = api_key or self._settings.api_key
        return {"X-API-Key": key} if key else {}

    def _attempt_order(self) -> list[str]:
        """The endpoints to try, starting at the next one in the rotation so
        consecutive calls land on different load balancers, then continuing
        through the rest as failover."""
        endpoints = self._settings.endpoints
        if not endpoints:
            return []
        start = next(_cursor) % len(endpoints)
        return [endpoints[(start + i) % len(endpoints)] for i in range(len(endpoints))]

    async def _send(self, base_url: str, method: str, path: str, **kwargs) -> httpx.Response:
        async with httpx.AsyncClient(
            base_url=base_url,
            timeout=self._settings.request_timeout_seconds,
            verify=self._settings.api_verify_ssl,
        ) as client:
            return await client.request(method, path, **kwargs)

    @staticmethod
    def _api_error(response: httpx.Response) -> UsecodeAgentApiError:
        detail = response.text
        try:
            detail = response.json().get("detail", detail)
        except ValueError:
            pass
        return UsecodeAgentApiError(response.status_code, detail)

    async def _request(self, method: str, path: str, **kwargs) -> httpx.Response:
        order = self._attempt_order()
        if not order:
            raise UsecodeAgentUnreachableError([], None)

        last_error: Exception | None = None
        for attempt, base_url in enumerate(order):
            is_last = attempt == len(order) - 1
            try:
                response = await self._send(base_url, method, path, **kwargs)
            except _UNREACHABLE as exc:
                # This load balancer is down; the request never left, so
                # try the next one.
                last_error = exc
                if is_last:
                    raise UsecodeAgentUnreachableError(order, exc) from exc
                continue

            if response.is_error:
                error = self._api_error(response)
                if (
                    not is_last
                    and response.status_code in _GATEWAY_STATUSES
                    and method.upper() in _REPEATABLE_METHODS
                ):
                    last_error = error
                    continue
                raise error
            return response

        # Only reachable if every endpoint answered with a gateway status.
        raise last_error if last_error else UsecodeAgentUnreachableError(order, None)

    async def request_otp(self, phone: str) -> dict:
        response = await self._request(
            "POST", "/auth/otp/request", json={"phone": phone}
        )
        return response.json()

    async def verify_otp(self, phone: str, code: str) -> dict:
        response = await self._request(
            "POST", "/auth/otp/verify", json={"phone": phone, "code": code}
        )
        return response.json()

    async def me(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/auth/me", headers=self._headers(api_key)
        )
        return response.json()

    async def logout(self, api_key: str | None = None) -> None:
        await self._request("POST", "/auth/logout", headers=self._headers(api_key))

    async def create_api_key(
        self, label: str = "", api_key: str | None = None
    ) -> dict:
        response = await self._request(
            "POST",
            "/auth/api-keys",
            json={"label": label},
            headers=self._headers(api_key),
        )
        return response.json()

    async def list_api_keys(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/auth/api-keys", headers=self._headers(api_key)
        )
        return response.json()

    async def revoke_api_key(self, key_id: str, api_key: str | None = None) -> None:
        await self._request(
            "DELETE", f"/auth/api-keys/{key_id}", headers=self._headers(api_key)
        )

    async def health(self) -> dict:
        response = await self._request("GET", "/health")
        return response.json()

    async def health_all(self) -> list[dict]:
        """Check every configured endpoint rather than just the next one in
        the rotation, so a single dead load balancer is visible instead of
        being silently failed over. The API names the node that answered
        (`node`), which is also how to see Caddy spreading requests over
        api-1/api-2."""
        results: list[dict] = []
        for base_url in self._settings.endpoints:
            entry: dict = {"endpoint": base_url}
            try:
                response = await self._send(base_url, "GET", "/health")
            except httpx.HTTPError as exc:
                entry.update(reachable=False, error=str(exc))
            else:
                if response.is_error:
                    error = self._api_error(response)
                    entry.update(
                        reachable=False,
                        status_code=error.status_code,
                        error=error.detail,
                    )
                else:
                    entry.update(reachable=True, **response.json())
            results.append(entry)
        return results

    async def model_options(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/models/options", headers=self._headers(api_key)
        )
        return response.json()

    async def model_status(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/models/status", headers=self._headers(api_key)
        )
        return response.json()

    async def model_start(self, api_key: str | None = None, **overrides) -> dict:
        payload = {k: v for k, v in overrides.items() if v is not None}
        response = await self._request(
            "POST", "/models/start", json=payload, headers=self._headers(api_key)
        )
        return response.json()

    async def model_stop(self, api_key: str | None = None) -> None:
        await self._request(
            "POST", "/models/stop", headers=self._headers(api_key)
        )

    async def set_provider_credentials(
        self, provider: str, credentials: dict, api_key: str | None = None
    ) -> dict:
        response = await self._request(
            "PUT",
            f"/providers/{provider}/credentials",
            json={"credentials": credentials},
            headers=self._headers(api_key),
        )
        return response.json()

    async def provider_credentials_status(
        self, provider: str, api_key: str | None = None
    ) -> dict:
        response = await self._request(
            "GET", f"/providers/{provider}/credentials", headers=self._headers(api_key)
        )
        return response.json()

    async def delete_provider_credentials(
        self, provider: str, api_key: str | None = None
    ) -> None:
        await self._request(
            "DELETE", f"/providers/{provider}/credentials", headers=self._headers(api_key)
        )

    async def list_provider_credentials(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/providers/credentials", headers=self._headers(api_key)
        )
        return response.json()

    async def list_servers(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/servers", headers=self._headers(api_key)
        )
        return response.json()

    async def list_server_types(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/servers/types", headers=self._headers(api_key)
        )
        return response.json()

    async def get_server(self, server_id: str, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", f"/servers/{server_id}", headers=self._headers(api_key)
        )
        return response.json()

    async def create_server(
        self,
        name: str,
        type: str,
        image: str = "ubuntu-24.04",
        ssh_keys: list[str] | None = None,
        api_key: str | None = None,
    ) -> dict:
        payload = {
            "name": name,
            "type": type,
            "image": image,
            "ssh_keys": ssh_keys or [],
        }
        response = await self._request(
            "POST",
            "/servers",
            json=payload,
            headers=self._headers(api_key),
        )
        return response.json()

    async def delete_server(self, server_id: str, api_key: str | None = None) -> dict:
        response = await self._request(
            "DELETE", f"/servers/{server_id}", headers=self._headers(api_key)
        )
        return response.json()

    async def get_task(self, task_id: str, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", f"/tasks/{task_id}", headers=self._headers(api_key)
        )
        return response.json()

    async def list_tasks(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "GET", "/tasks", headers=self._headers(api_key)
        )
        return response.json()

    async def sync_servers(self, api_key: str | None = None) -> dict:
        response = await self._request(
            "POST", "/servers/sync", headers=self._headers(api_key)
        )
        return response.json()

    async def list_catalog(
        self,
        provider: str | None = None,
        kind: str | None = None,
        api_key: str | None = None,
    ) -> dict:
        params = {k: v for k, v in {"provider": provider, "kind": kind}.items() if v}
        response = await self._request(
            "GET", "/servers/catalog", params=params, headers=self._headers(api_key)
        )
        return response.json()
