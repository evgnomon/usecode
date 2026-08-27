from functools import lru_cache

from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="USECODE_AGENT_BOT_", env_file=".env", extra="ignore"
    )

    # The Caddy load balancers in front of usecode-agent-api, each routing /api/*
    # to the API instances behind it. Requests are spread over these
    # round-robin, and one that can't reach an endpoint is retried against
    # the next.
    #
    # This is the tier *above* Caddy's own load balancing: Caddy already
    # spreads requests over api-1/api-2, but a client pinned to a single
    # Caddy goes down with it. See deploy/compose.yml.
    api_base_urls: list[str] = [
        "https://localhost:4430/api",
        "https://localhost:4431/api",
    ]

    # Single-endpoint override. Set USECODE_AGENT_BOT_API_BASE_URL to talk to one
    # specific address (a remote deployment, or a bare usecode-agent-api with no
    # Caddy in front) — it replaces the list above rather than adding to it,
    # so the bot then has exactly the one endpoint asked for.
    api_base_url: str | None = None

    # API key issued by POST /auth/otp/verify. Lets the bot act as an already
    # logged-in usecode agent client instead of running the OTP flow on every call.
    api_key: str | None = None

    request_timeout_seconds: float = 10.0

    # Verify the API server's TLS certificate. Caddy issues a self-signed
    # cert for local/non-public deployments, so set this to False in your
    # local .env if you hit CERTIFICATE_VERIFY_FAILED talking to localhost.
    api_verify_ssl: bool = True

    # Path to the compose file used to run usecode agent locally. Defaults to
    # deploy/compose.yml at the root of this repo checkout.
    compose_file: str | None = None

    # Container tooling to drive the compose file with: "podman" (uses
    # podman-compose, matching deploy/push.sh and deploy/pull.sh) or "docker"
    # (uses `docker compose`).
    container_cli: str = "podman"

    @property
    def endpoints(self) -> list[str]:
        """The base URLs to spread requests over, in configuration order."""
        if self.api_base_url:
            return [self.api_base_url]
        return list(self.api_base_urls)


@lru_cache
def get_settings() -> Settings:
    return Settings()
