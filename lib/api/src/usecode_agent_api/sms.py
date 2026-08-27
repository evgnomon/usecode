import logging
from typing import Protocol

import httpx

from .config import Settings

logger = logging.getLogger("usecode_agent_api.sms")


class SmsSender(Protocol):
    async def send(self, phone: str, code: str) -> None: ...


class ConsoleSmsSender:
    """Logs the OTP instead of sending it. Used when no SMS provider is configured."""

    async def send(self, phone: str, code: str) -> None:
        logger.info("SMS to %s: your usecode agent login code is %s", phone, code)


class HttpSmsSender:
    """Sends the OTP through a generic HTTP SMS provider configured via env vars."""

    def __init__(self, api_url: str, api_key: str, sender_name: str) -> None:
        self._api_url = api_url
        self._api_key = api_key
        self._sender_name = sender_name

    async def send(self, phone: str, code: str) -> None:
        async with httpx.AsyncClient(timeout=10) as client:
            response = await client.post(
                self._api_url,
                headers={"Authorization": f"Bearer {self._api_key}"},
                json={
                    "to": phone,
                    "sender": self._sender_name,
                    "message": f"Your usecode agent login code is {code}",
                },
            )
            response.raise_for_status()


def get_sms_sender(settings: Settings) -> SmsSender:
    if settings.sms_api_url and settings.sms_api_key:
        return HttpSmsSender(
            settings.sms_api_url, settings.sms_api_key, settings.sms_sender_name
        )
    return ConsoleSmsSender()
