import secrets
import time

from fastapi import APIRouter, Depends, Header, HTTPException, status

from ..config import Settings, get_settings
from ..models import (
    ApiKeyCreateIn,
    ApiKeyCreateOut,
    ApiKeyListOut,
    ApiKeyOut,
    AuthTokenOut,
    MeOut,
    OtpRequestIn,
    OtpRequestOut,
    OtpVerifyIn,
)
from ..security import get_current_client
from ..sms import get_sms_sender
from ..store import ApiKeyRecord, OtpRecord, store

router = APIRouter(prefix="/auth", tags=["auth"])


def _generate_code(length: int) -> str:
    return "".join(str(secrets.randbelow(10)) for _ in range(length))


@router.post("/otp/request", response_model=OtpRequestOut)
async def request_otp(
    payload: OtpRequestIn, settings: Settings = Depends(get_settings)
) -> OtpRequestOut:
    now = time.time()
    existing = await store.get_otp(payload.phone)
    if existing and existing.resend_after > now:
        retry_in = int(existing.resend_after - now)
        raise HTTPException(
            status.HTTP_429_TOO_MANY_REQUESTS,
            detail=f"Please wait {retry_in}s before requesting another code",
        )

    code = _generate_code(settings.otp_length)
    record = OtpRecord(
        code=code,
        expires_at=now + settings.otp_ttl_seconds,
        resend_after=now + settings.otp_resend_cooldown_seconds,
    )
    await store.put_otp(payload.phone, record)

    sender = get_sms_sender(settings)
    await sender.send(payload.phone, code)

    return OtpRequestOut(
        phone=payload.phone,
        expires_in=settings.otp_ttl_seconds,
        resend_after=settings.otp_resend_cooldown_seconds,
        debug_code=code if settings.debug_expose_otp else None,
    )


@router.post("/otp/verify", response_model=AuthTokenOut)
async def verify_otp(
    payload: OtpVerifyIn, settings: Settings = Depends(get_settings)
) -> AuthTokenOut:
    record = await store.get_otp(payload.phone)
    if record is None:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, detail="Request a code first")

    now = time.time()
    if record.expires_at < now:
        await store.clear_otp(payload.phone)
        raise HTTPException(
            status.HTTP_400_BAD_REQUEST, detail="Code expired, request a new one"
        )

    if record.attempts >= settings.otp_max_attempts:
        await store.clear_otp(payload.phone)
        raise HTTPException(
            status.HTTP_429_TOO_MANY_REQUESTS,
            detail="Too many attempts, request a new code",
        )

    if not await store.check_otp_code(payload.phone, payload.code):
        await store.increment_attempts(payload.phone)
        raise HTTPException(status.HTTP_400_BAD_REQUEST, detail="Incorrect code")

    await store.clear_otp(payload.phone)
    user_id, _partition_key = await store.get_or_create_user(payload.phone)
    api_key = await store.issue_api_key(user_id, label="login")
    return AuthTokenOut(api_key=api_key, phone=payload.phone)


@router.get("/me", response_model=MeOut)
async def me(client: ApiKeyRecord = Depends(get_current_client)) -> MeOut:
    return MeOut(phone=client.phone, created_at=client.created_at)


@router.post("/logout", status_code=status.HTTP_204_NO_CONTENT)
async def logout(x_api_key: str | None = Header(default=None)) -> None:
    if x_api_key:
        await store.revoke_api_key(x_api_key)
    return None


@router.post("/api-keys", response_model=ApiKeyCreateOut)
async def create_api_key(
    payload: ApiKeyCreateIn,
    client: ApiKeyRecord = Depends(get_current_client),
) -> ApiKeyCreateOut:
    api_key = await store.issue_api_key(client.user_id, label=payload.label)
    created = await store.get_api_key(api_key)
    assert created is not None
    return ApiKeyCreateOut(
        id=created.id,
        api_key=api_key,
        label=created.label,
        created_at=created.created_at,
    )


@router.get("/api-keys", response_model=ApiKeyListOut)
async def list_api_keys(
    client: ApiKeyRecord = Depends(get_current_client),
) -> ApiKeyListOut:
    keys = await store.list_api_keys(client.user_id)
    return ApiKeyListOut(
        api_keys=[
            ApiKeyOut(
                id=key.id,
                label=key.label,
                created_at=key.created_at,
                last_used_at=key.last_used_at,
            )
            for key in keys
        ]
    )


@router.delete("/api-keys/{key_id}", status_code=status.HTTP_204_NO_CONTENT)
async def revoke_api_key(
    key_id: str,
    client: ApiKeyRecord = Depends(get_current_client),
) -> None:
    # The id of an API key is its hash — what routes to the instance
    # holding it. See db_models.ApiKey.
    revoked = await store.revoke_api_key_for_user(client.user_id, key_id)
    if not revoked:
        raise HTTPException(status.HTTP_404_NOT_FOUND, detail="API key not found")
    return None
