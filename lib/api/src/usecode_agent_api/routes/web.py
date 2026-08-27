import secrets
import time
from pathlib import Path

from fastapi import APIRouter, Depends, Form, HTTPException, Request, Response
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates
from jinja2 import Environment, FileSystemLoader, select_autoescape

from ..chat_store import chat_store
from ..config import Settings, get_settings
from ..models import normalize_phone
from ..sms import get_sms_sender
from ..store import OtpRecord, WebSessionRecord, store

router = APIRouter(tags=["web"])
_templates_dir = str(Path(__file__).resolve().parents[1] / "templates")
templates = Jinja2Templates(
    env=Environment(
        loader=FileSystemLoader(_templates_dir),
        autoescape=select_autoescape(enabled_extensions=("html.jinja2",)),
    )
)
AUTH_COOKIE_NAME = "usecode_agent_web_session"


def _format_time(timestamp: float) -> str:
    return time.strftime("%H:%M", time.localtime(timestamp))


def _time_ago(timestamp: float) -> str:
    diff_min = max(0, round((time.time() - timestamp) / 60))
    if diff_min < 1:
        return "now"
    if diff_min < 60:
        return f"{diff_min}m"
    diff_hr = round(diff_min / 60)
    if diff_hr < 24:
        return f"{diff_hr}h"
    return f"{round(diff_hr / 24)}d"


def _enrich_messages(messages: list[dict[str, object]]) -> list[dict[str, object]]:
    enriched: list[dict[str, object]] = []
    for message in messages:
        payload = dict(message)
        timestamp = float(payload["ts"])
        payload["time"] = _format_time(timestamp)
        payload["ago"] = _time_ago(timestamp)
        enriched.append(payload)
    return enriched


async def _require_web_client(request: Request) -> WebSessionRecord:
    api_key = request.cookies.get(AUTH_COOKIE_NAME)
    if not api_key:
        raise HTTPException(status_code=401)
    client = await store.get_web_session(api_key)
    if not client:
        raise HTTPException(status_code=401)
    return client


def _render_shell(request: Request, client: WebSessionRecord) -> HTMLResponse:
    agents = chat_store.get_agents()
    previews: dict[str, dict[str, object]] = {}
    for agent in agents:
        messages = chat_store.get_messages(client.phone, agent["id"])
        if messages:
            preview = dict(messages[-1])
            preview["ago"] = _time_ago(float(preview["ts"]))
            previews[agent["id"]] = preview
    return templates.TemplateResponse(
        request,
        "shell.html.jinja2",
        {
            "agents": agents,
            "previews": previews,
            "active_agent_id": None,
        },
    )


@router.get("/", response_class=HTMLResponse)
async def index(request: Request) -> HTMLResponse:
    api_key = request.cookies.get(AUTH_COOKIE_NAME)
    client = await store.get_web_session(api_key) if api_key else None
    if client:
        return _render_shell(request, client)
    return templates.TemplateResponse(request, "login.html.jinja2", {})


@router.get("/web/auth/reset", response_class=HTMLResponse)
async def reset_auth_step(request: Request) -> HTMLResponse:
    return templates.TemplateResponse(
        request, "partials/login_phone_form.html.jinja2", {}
    )


@router.post("/web/auth/request-otp", response_class=HTMLResponse)
async def request_otp_form(
    request: Request,
    phone: str = Form(...),
    settings: Settings = Depends(get_settings),
) -> HTMLResponse:
    try:
        normalized_phone = normalize_phone(phone)
    except ValueError as exc:
        return templates.TemplateResponse(
            request,
            "partials/login_phone_form.html.jinja2",
            {
                "phone": phone,
                "error": str(exc),
            },
            status_code=400,
        )

    now = time.time()
    existing = await store.get_otp(normalized_phone)
    if existing and existing.resend_after > now:
        retry_in = int(existing.resend_after - now)
        return templates.TemplateResponse(
            request,
            "partials/login_phone_form.html.jinja2",
            {
                "phone": normalized_phone,
                "error": f"Please wait {retry_in}s before requesting another code",
            },
            status_code=429,
        )

    code = "".join(str(secrets.randbelow(10)) for _ in range(settings.otp_length))
    record = OtpRecord(
        code=code,
        expires_at=now + settings.otp_ttl_seconds,
        resend_after=now + settings.otp_resend_cooldown_seconds,
    )
    await store.put_otp(normalized_phone, record)

    sender = get_sms_sender(settings)
    await sender.send(normalized_phone, code)

    return templates.TemplateResponse(
        request,
        "partials/login_code_form.html.jinja2",
        {
            "phone": normalized_phone,
            "debug_code": code if settings.debug_expose_otp else None,
        },
    )


@router.post("/web/auth/verify-otp", response_class=HTMLResponse)
async def verify_otp_form(
    request: Request,
    phone: str = Form(...),
    code: str = Form(...),
    settings: Settings = Depends(get_settings),
) -> Response:
    try:
        normalized_phone = normalize_phone(phone)
    except ValueError as exc:
        return templates.TemplateResponse(
            request,
            "partials/login_phone_form.html.jinja2",
            {
                "phone": phone,
                "error": str(exc),
            },
            status_code=400,
        )

    cleaned_code = code.strip()
    record = await store.get_otp(normalized_phone)
    if record is None:
        return templates.TemplateResponse(
            request,
            "partials/login_code_form.html.jinja2",
            {
                "phone": normalized_phone,
                "error": "Request a code first",
            },
            status_code=400,
        )

    now = time.time()
    if record.expires_at < now:
        await store.clear_otp(normalized_phone)
        return templates.TemplateResponse(
            request,
            "partials/login_code_form.html.jinja2",
            {
                "phone": normalized_phone,
                "error": "Code expired, request a new one",
            },
            status_code=400,
        )

    if record.attempts >= settings.otp_max_attempts:
        await store.clear_otp(normalized_phone)
        return templates.TemplateResponse(
            request,
            "partials/login_code_form.html.jinja2",
            {
                "phone": normalized_phone,
                "error": "Too many attempts, request a new code",
            },
            status_code=429,
        )

    if not cleaned_code.isdigit() or not await store.check_otp_code(
        normalized_phone, cleaned_code
    ):
        await store.increment_attempts(normalized_phone)
        return templates.TemplateResponse(
            request,
            "partials/login_code_form.html.jinja2",
            {
                "phone": normalized_phone,
                "error": "Incorrect code",
            },
            status_code=400,
        )

    await store.clear_otp(normalized_phone)
    user_id, _partition_key = await store.get_or_create_user(normalized_phone)
    api_key = await store.issue_api_key(user_id, label="web")
    api_key_record = await store.get_api_key(api_key)
    assert api_key_record is not None
    web_session = await store.issue_web_session(user_id, api_key_record.id)

    response = Response(status_code=204, headers={"HX-Redirect": "/"})
    response.set_cookie(
        AUTH_COOKIE_NAME,
        web_session,
        httponly=True,
        secure=False,
        samesite="lax",
        max_age=60 * 60 * 24 * 30,
    )
    return response


@router.post("/web/auth/logout")
async def logout_form(request: Request) -> Response:
    session_token = request.cookies.get(AUTH_COOKIE_NAME)
    if session_token:
        session = await store.get_web_session(session_token)
        if session:
            await store.revoke_api_key_for_user(session.user_id, session.api_key_hash)
        await store.revoke_web_session(session_token)

    response = Response(status_code=204, headers={"HX-Redirect": "/"})
    response.delete_cookie(AUTH_COOKIE_NAME)
    return response


@router.get("/web/chat/{agent_id}", response_class=HTMLResponse)
async def chat_panel(
    request: Request,
    agent_id: str,
    client: WebSessionRecord = Depends(_require_web_client),
) -> HTMLResponse:
    agent = chat_store.get_agent(agent_id)
    if not agent:
        raise HTTPException(status_code=404, detail="Agent not found")

    messages = _enrich_messages(chat_store.get_messages(client.phone, agent_id))
    return templates.TemplateResponse(
        request,
        "partials/chat_panel.html.jinja2",
        {
            "agent": agent,
            "messages": messages,
            "active_agent_id": agent_id,
        },
    )


@router.post("/web/chat/{agent_id}/messages", response_class=HTMLResponse)
async def send_message(
    request: Request,
    agent_id: str,
    message: str = Form(...),
    client: WebSessionRecord = Depends(_require_web_client),
) -> HTMLResponse:
    if not chat_store.get_agent(agent_id):
        raise HTTPException(status_code=404, detail="Agent not found")

    text = message.strip()
    if text:
        chat_store.send_user_message(client.phone, agent_id, text)
        chat_store.add_agent_reply(client.phone, agent_id)

    messages = _enrich_messages(chat_store.get_messages(client.phone, agent_id))
    return templates.TemplateResponse(
        request,
        "partials/chat_messages.html.jinja2",
        {
            "messages": messages,
        },
    )
