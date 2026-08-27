import asyncio
import logging
from contextlib import asynccontextmanager
from pathlib import Path

from alembic import command
from alembic.config import Config
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles

from . import db, tasks
from .config import get_settings
from .routes.auth import router as auth_router
from .routes.models import router as models_router
from .routes.providers import router as providers_router
from .routes.servers import router as servers_router
from .routes.tasks import router as tasks_router
from .routes.web import router as web_router

_ALEMBIC_INI = Path(__file__).resolve().parents[2] / "alembic.ini"

# How often to resume tasks that are parked waiting on a slow provider-side
# operation (e.g. "wait for a server deletion to finish"). See tasks.py and
# AGENTS.md ("Provider resources and tasks").
_TASK_SWEEP_INTERVAL_SECONDS = 5


def run_migrations() -> None:
    """Bring every database instance to head. All instances carry the
    identical schema — what differs is which rows are on which — so the same
    migration set is applied to each, taken from static configuration since
    the shard-mapping table can't be read before it exists.

    Several API instances boot at once behind the load balancer and would
    otherwise race here, so each upgrade runs under a PostgreSQL advisory
    lock held for the migration transaction (see migrations/env.py); the
    instances that lose the race simply find the database already at head."""
    for partition_key, url in db.configured_urls(get_settings()).items():
        logging.getLogger(__name__).info(
            "Migrating database for partition %s", partition_key or "<main>"
        )
        alembic_cfg = Config(str(_ALEMBIC_INI))
        alembic_cfg.set_main_option("sqlalchemy.url", url)
        command.upgrade(alembic_cfg, "head")


async def _sweep_tasks_forever() -> None:
    while True:
        await asyncio.sleep(_TASK_SWEEP_INTERVAL_SECONDS)
        try:
            await tasks.sweep()
        except Exception:
            logging.getLogger(__name__).exception("Task sweep failed")


@asynccontextmanager
async def lifespan(app: FastAPI):
    settings = get_settings()
    logging.getLogger(__name__).info(
        "Starting API instance %r", settings.node_name
    )
    await asyncio.to_thread(run_migrations)
    await db.connect(settings)
    await db.seed_shard_ranges(settings)
    sweeper = asyncio.create_task(_sweep_tasks_forever())
    yield
    sweeper.cancel()
    await db.disconnect()


def create_app() -> FastAPI:
    settings = get_settings()
    app = FastAPI(title="usecode agent API", version="0.1.0", lifespan=lifespan)

    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.cors_origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    static_dir = Path(__file__).resolve().parent / "static"
    app.mount("/static", StaticFiles(directory=static_dir), name="static")

    app.include_router(auth_router)
    app.include_router(models_router)
    app.include_router(providers_router)
    app.include_router(servers_router)
    app.include_router(tasks_router)
    app.include_router(web_router)

    @app.get("/health")
    async def health() -> dict[str, str]:
        # Caddy load-balances over the nodes and health-checks this
        # endpoint, so name the node in the response — it's the simplest way
        # to see which one served a given request.
        return {"status": "ok", "node": settings.node_name}

    return app


app = create_app()
