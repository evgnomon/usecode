# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

from pathlib import Path

from fastapi import FastAPI, Request
from fastapi.responses import HTMLResponse, PlainTextResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from jinja2 import Environment, FileSystemLoader, select_autoescape

from . import content

_here = Path(__file__).resolve().parent
templates = Jinja2Templates(
    env=Environment(
        loader=FileSystemLoader(str(_here / "templates")),
        autoescape=select_autoescape(enabled_extensions=("html.jinja2",)),
    )
)
templates.env.globals["repo_url"] = content.REPO_URL
templates.env.globals["license_name"] = content.LICENSE_NAME
templates.env.globals["license_url"] = content.LICENSE_URL
templates.env.globals["source_license_url"] = content.SOURCE_LICENSE_URL

app = FastAPI(title="usecode.dev", docs_url=None, redoc_url=None)
app.mount("/static", StaticFiles(directory=str(_here / "static")), name="static")


@app.get("/", response_class=HTMLResponse)
async def index(request: Request) -> HTMLResponse:
    return templates.TemplateResponse(
        request,
        "index.html.jinja2",
        {
            "page": "home",
            "features": content.FEATURES,
            "providers": content.PROVIDERS,
            "editions": content.EDITIONS,
            "prompts": content.EXAMPLE_PROMPTS,
            "license_points": content.LICENSE_POINTS,
            "faqs": content.FAQS,
        },
    )


@app.get("/getting-started", response_class=HTMLResponse)
async def getting_started(request: Request) -> HTMLResponse:
    return templates.TemplateResponse(
        request,
        "getting-started.html.jinja2",
        {
            "page": "getting-started",
            "steps": content.INSTALL_STEPS,
            "agents": content.AGENTS,
            "prompts": content.EXAMPLE_PROMPTS,
        },
    )


@app.get("/team", response_class=HTMLResponse)
async def team(request: Request) -> HTMLResponse:
    return templates.TemplateResponse(
        request,
        "team.html.jinja2",
        {
            "page": "team",
            "editions": content.EDITIONS,
            "hosted_values": content.HOSTED_VALUES,
            "self_host_steps": content.SELF_HOST_STEPS,
            "license_points": content.LICENSE_POINTS,
            "faqs": content.FAQS,
        },
    )


@app.get("/healthz", response_class=PlainTextResponse)
async def healthz() -> str:
    return "ok"
