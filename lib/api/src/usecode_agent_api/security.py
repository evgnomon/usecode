from fastapi import Header, HTTPException, status

from .store import ApiKeyRecord, store


async def get_current_client(
    x_api_key: str | None = Header(default=None),
) -> ApiKeyRecord:
    if not x_api_key:
        raise HTTPException(
            status.HTTP_401_UNAUTHORIZED, detail="Missing X-API-Key header"
        )
    record = await store.get_api_key(x_api_key)
    if record is None:
        raise HTTPException(
            status.HTTP_401_UNAUTHORIZED, detail="Invalid or expired API key"
        )
    return record
