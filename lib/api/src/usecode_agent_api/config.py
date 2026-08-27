from functools import lru_cache

from pydantic import field_validator, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="USECODE_AGENT_", env_file=".env", extra="ignore"
    )

    # -- Instance identity -------------------------------------------------
    # Name of *this* API instance, e.g. "api-1" or "worker-1". Required: the
    # process refuses to start without it, because a task's assignee is a
    # node name (see db_models.Task) — and the assignee is also the task's
    # address, the key `tasks` is partitioned on. An unnamed instance could
    # neither claim work nor say where its work is stored, so running one
    # would silently strand tasks.
    node_name: str

    otp_length: int = 6
    otp_ttl_seconds: int = 300
    otp_resend_cooldown_seconds: int = 60
    otp_max_attempts: int = 5

    # When true, the OTP code is echoed back in the request-otp response instead of
    # requiring a real SMS provider. Meant for local development only.
    debug_expose_otp: bool = True

    # Optional generic HTTP SMS provider. Left unset, the server just logs the code.
    sms_api_url: str | None = None
    sms_api_key: str | None = None
    sms_sender_name: str = "usecode agent"

    cors_origins: list[str] = ["http://localhost:5173", "http://127.0.0.1:5173"]

    # -- Databases ---------------------------------------------------------
    # Connection string (SQLAlchemy async form) for the **main/first**
    # PostgreSQL instance — the one a null partition key resolves to, and
    # the one holding the `shard_ranges` table that says which instance owns
    # which virtual shards. e.g. postgresql+asyncpg://user:pass@host:5432/usecode_agent
    database_url: str = "postgresql+asyncpg://usecode_agent:usecode_agent@localhost:5432/usecode_agent"

    # The *other* database instances, keyed by partition key, as
    # "host:port/database" — e.g. {"b": "postgres-2:5432/usecode_agent"}. Every
    # shard runs the identical schema under the identical role, so the
    # driver and credentials are reused from `database_url` rather than
    # repeated here. This is bootstrap configuration: it is what the
    # `shard_ranges` table on the main database is seeded from the *first*
    # time a deployment boots (the virtual shards split evenly across these
    # instances plus the main one) and what migrations are applied to. At
    # runtime, resolution reads that table, not this setting.
    shards: dict[str, str] = {}

    # Container tooling used to run the AI model (llama-server) container:
    # "podman" (default) or "docker".
    model_container_cli: str = "podman"

    # Used to encrypt per-user secrets (e.g. Hetzner API tokens) at rest.
    # Change this in production and keep it stable, rotating it invalidates
    # every stored secret.
    secret_key: str = "dev-insecure-secret-key-change-me"

    @field_validator("node_name")
    @classmethod
    def _strip_name(cls, value: str) -> str:
        return value.strip()

    @model_validator(mode="after")
    def _check_node_name(self) -> "Settings":
        # Enforced here rather than by the field being merely required,
        # because USECODE_AGENT_NODE_NAME="" would otherwise satisfy "required"
        # while being just as unusable as an absent one: every such node's
        # tasks would be assigned to "" and would all hash to the same
        # bucket, so the nodes would sweep each other's work.
        if not self.node_name:
            raise ValueError(
                "USECODE_AGENT_NODE_NAME must be set to a non-empty node name, e.g. 'api-1'. "
                "A task's assignee is a node name and is what addresses the task, so "
                "an unnamed node could neither claim work nor find it again."
            )
        return self


@lru_cache
def get_settings() -> Settings:
    return Settings()
