from fastapi import APIRouter, Depends, HTTPException, status

from ..models import TaskListOut, TaskOut
from ..security import get_current_client
from ..store import ApiKeyRecord, TaskRecord, store

router = APIRouter(prefix="/tasks", tags=["tasks"], dependencies=[Depends(get_current_client)])


def _to_out(task: TaskRecord) -> TaskOut:
    return TaskOut(
        id=task.id,
        kind=task.kind,
        assignee=task.assignee,
        state=task.state,
        resources=task.resources,
        error=task.error,
        created_at=task.created_at,
        updated_at=task.updated_at,
    )


@router.get("", response_model=TaskListOut)
async def list_tasks(client: ApiKeyRecord = Depends(get_current_client)) -> TaskListOut:
    """List the caller's in-flight background tasks (e.g. the
    create_server/delete_server workflows started by POST/DELETE /servers)
    that haven't finished yet — a task disappears once done. See
    AGENTS.md ("Provider resources and tasks")."""
    tasks = await store.list_user_tasks(client.user_id)
    return TaskListOut(tasks=[_to_out(task) for task in tasks])


@router.get("/{task_id}", response_model=TaskOut)
async def get_task(task_id: str, client: ApiKeyRecord = Depends(get_current_client)) -> TaskOut:
    # By id alone a task addresses nothing — `tasks` is partitioned on the
    # assignee — so this goes through the caller's own `user_tasks` index,
    # which is both what says where the row is and what proves it is theirs.
    task = await store.get_user_task(client.user_id, task_id)
    if task is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, detail="Task not found")
    return _to_out(task)
