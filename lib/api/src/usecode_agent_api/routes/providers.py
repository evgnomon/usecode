from fastapi import APIRouter, Depends, HTTPException, status

from .. import crypto
from ..config import Settings, get_settings
from ..models import (
    ProviderCredentialsIn,
    ProviderCredentialsListOut,
    ProviderCredentialsStatusOut,
)
from ..provider_credentials import check_known_provider
from ..providers import PROVIDERS, InvalidCredentialsError
from ..providers import validate_credentials as provider_validate_credentials
from ..security import get_current_client
from ..store import ApiKeyRecord, store

router = APIRouter(
    prefix="/providers", tags=["providers"], dependencies=[Depends(get_current_client)]
)


@router.put("/{provider}/credentials", response_model=ProviderCredentialsStatusOut)
async def set_credentials(
    provider: str,
    payload: ProviderCredentialsIn,
    client: ApiKeyRecord = Depends(get_current_client),
    settings: Settings = Depends(get_settings),
) -> ProviderCredentialsStatusOut:
    check_known_provider(provider)
    try:
        provider_validate_credentials(provider, payload.credentials)
    except InvalidCredentialsError as exc:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc

    credentials_encrypted = crypto.encrypt_json(settings, payload.credentials)
    await store.set_provider_credentials(
        client.partition_key, client.user_id, provider, credentials_encrypted
    )
    return ProviderCredentialsStatusOut(provider=provider, configured=True)


@router.get("/{provider}/credentials", response_model=ProviderCredentialsStatusOut)
async def get_credentials_status(
    provider: str,
    client: ApiKeyRecord = Depends(get_current_client),
) -> ProviderCredentialsStatusOut:
    check_known_provider(provider)
    credentials_encrypted = await store.get_provider_credentials(
        client.partition_key, client.user_id, provider
    )
    return ProviderCredentialsStatusOut(
        provider=provider, configured=credentials_encrypted is not None
    )


@router.delete("/{provider}/credentials", status_code=status.HTTP_204_NO_CONTENT)
async def delete_credentials(
    provider: str,
    client: ApiKeyRecord = Depends(get_current_client),
) -> None:
    check_known_provider(provider)
    await store.delete_provider_credentials(
        client.partition_key, client.user_id, provider
    )
    return None


@router.get("/credentials", response_model=ProviderCredentialsListOut)
async def list_credentials_status(
    client: ApiKeyRecord = Depends(get_current_client),
) -> ProviderCredentialsListOut:
    configured = set(
        await store.list_configured_providers(client.partition_key, client.user_id)
    )
    return ProviderCredentialsListOut(
        providers=[
            ProviderCredentialsStatusOut(provider=provider, configured=provider in configured)
            for provider in sorted(PROVIDERS)
        ]
    )
