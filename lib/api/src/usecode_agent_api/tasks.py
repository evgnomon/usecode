"""Task engine: resumable workflows over provider-owned resources.

See AGENTS.md ("Provider resources and tasks") for the architecture this
implements. In short: a `Task` row is a suspended async function. `state`
is the step it is parked at; a step handler runs, does one unit of work
(usually one provider API call or one poll), and returns either the next
state to suspend at or `DONE` once the workflow has finished. A step that
finishes a task is responsible for applying the effect to our own resource
tables (e.g. deleting the `servers` row) *before* returning `DONE` — the
engine then deletes the task row itself, so a task never outlives the
resource mutation it exists to perform.

Step handlers are registered per (kind, state) with `@step(...)`. A given
`kind` (e.g. "delete_server") is a fixed sequence of named states, similar
to labelled steps in an async function that can await external events
between them.

Because the API runs as several named instances behind a load balancer,
every task has an **assignee**: the node name of the instance that picked
it up. Only the assignee sweeps it, so two instances never run the same
step concurrently — and because that is how tasks are read, it is also
where they are stored: `tasks` is partitioned on the assignee (see
`db_models.Task`), so an instance's whole sweep is one query on one
database rather than a walk across all of them. A task is addressed as
(assignee, id), and the assignee travels with the task id everywhere in
this module for that reason.

The rows a step *mutates* are a different matter: those belong to the
user, on the instance their owner hashes to. Every context therefore
carries both — `assignee` for the task itself, `user_partition_key` for
everything the handler touches on the user's behalf.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Awaitable, Callable

from . import db
from .config import get_settings
from .store import TaskRecord, store

DONE = None


@dataclass
class TaskContext:
    task_id: str
    # The node carrying this task, which is also where its row is (see the
    # module docstring); half of the task's address.
    assignee: str
    user_id: str
    # The database instance the *user's* rows live on — servers, provider
    # credentials, everything a step actually mutates. Not where the task
    # itself is: step handlers pass this to every partitioned store call
    # they make.
    user_partition_key: str | None
    resources: list
    payload: dict


StepFn = Callable[[TaskContext], Awaitable[tuple[str | None, dict | None]]]

_STEPS: dict[tuple[str, str], StepFn] = {}


def step(kind: str, state: str) -> Callable[[StepFn], StepFn]:
    """Register the handler that runs when a task of `kind` is parked at
    `state`. The handler returns `(next_state, payload)`: `next_state` is
    `DONE` (None) once the workflow is finished, otherwise the state to
    suspend at next; `payload` replaces the task's payload if given."""

    def register(fn: StepFn) -> StepFn:
        _STEPS[(kind, state)] = fn
        return fn

    return register


async def create_task(
    user_id: str,
    kind: str,
    initial_state: str,
    resources: list,
    payload: dict,
) -> TaskRecord:
    """Create a task assigned to *this* API instance — the one handling the
    request is the one that picked the work up, and it is the one that will
    carry it to completion. That assignee is also the task's address, so
    the row lands on the instance this node's name hashes to, wherever the
    user's own rows may be."""
    return await store.create_task(
        user_id,
        kind,
        get_settings().node_name,
        initial_state,
        resources,
        payload,
    )


async def advance(assignee: str, task_id: str) -> TaskRecord | None:
    """Run the task's current step once. Returns the task's new state, or
    None if the task is no longer around (finished, or never existed).

    Only the assignee calls this — it names the node whose database holds
    the task, so a caller that isn't that node has nothing to advance."""
    task = await store.get_task(assignee, task_id)
    if task is None:
        return None

    handler = _STEPS.get((task.kind, task.state))
    if handler is None:
        raise RuntimeError(
            f"No step handler for task kind={task.kind!r} state={task.state!r}"
        )

    ctx = TaskContext(
        task_id=task.id,
        assignee=assignee,
        user_id=task.user_id,
        # Resolved from the owner, not from where the task itself sits:
        # the two are unrelated instances now that tasks hash on assignee.
        user_partition_key=await db.partition_for_user(task.user_id),
        resources=task.resources,
        payload=task.payload,
    )

    try:
        next_state, payload = await handler(ctx)
    except Exception as exc:  # noqa: BLE001 — keep the task alive at its current state for retry
        await store.set_task_state(assignee, task.id, task.state, error=str(exc))
        return await store.get_task(assignee, task.id)

    if next_state is DONE:
        # The handler already applied its effect to the resource table(s);
        # the workflow is complete, so the task itself goes away.
        await store.delete_task(assignee, task.id)
        return None

    await store.set_task_state(assignee, task.id, next_state, payload=payload)
    return await store.get_task(assignee, task.id)


async def sweep() -> None:
    """Advance every one of *this* instance's outstanding tasks that isn't
    parked waiting on a fresh external event this tick. Meant to run on a
    periodic timer so tasks suspended on a slow provider operation (e.g.
    "wait for deletion to finish", which can take hours) eventually get
    resumed and completed without a request being in flight.

    A task's owner is its assignee, so each instance sweeps only what it
    picked up — and since `tasks` is partitioned on the assignee, this
    node's whole backlog is a single query against the single instance its
    name hashes to. No partition is enumerated and no other node's work is
    ever read."""
    node_name = get_settings().node_name
    for task in await store.list_tasks(node_name):
        await advance(node_name, task.id)
