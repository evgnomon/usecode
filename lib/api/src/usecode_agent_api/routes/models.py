from fastapi import APIRouter, Depends, HTTPException, status

from .. import model_process
from ..config import Settings, get_settings
from ..model_config import known_options
from ..model_process import ModelProcessError
from ..models import ModelOptionsOut, ModelStartIn, ModelStatusOut
from ..security import get_current_client

router = APIRouter(
    prefix="/models", tags=["models"], dependencies=[Depends(get_current_client)]
)


@router.get("/options", response_model=ModelOptionsOut)
async def get_options() -> ModelOptionsOut:
    return ModelOptionsOut(fields=known_options())


@router.get("/status", response_model=ModelStatusOut)
async def get_status(settings: Settings = Depends(get_settings)) -> ModelStatusOut:
    return ModelStatusOut(**model_process.status(settings))


@router.post("/start", response_model=ModelStatusOut)
async def start_model(
    payload: ModelStartIn,
    settings: Settings = Depends(get_settings),
) -> ModelStatusOut:
    try:
        result = model_process.start(settings, payload.model_dump(exclude_none=True))
    except ValueError as exc:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except ModelProcessError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    return ModelStatusOut(running=True, config=result["config"])


@router.post("/stop", status_code=status.HTTP_204_NO_CONTENT)
async def stop_model(
    settings: Settings = Depends(get_settings),
) -> None:
    try:
        model_process.stop(settings)
    except ModelProcessError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    return None
