import random
import threading
import time
from copy import deepcopy

AGENTS = [
    {
        "id": "assistant",
        "name": "General Assistant",
        "avatar": "🤖",
        "color": "#5b8def",
        "status": "online",
        "tagline": "Ask me anything",
    },
    {
        "id": "coder",
        "name": "Code Helper",
        "avatar": "💻",
        "color": "#3ecf8e",
        "status": "online",
        "tagline": "Debugging & code review",
    },
    {
        "id": "writer",
        "name": "Writing Coach",
        "avatar": "✍️",
        "color": "#f2994a",
        "status": "away",
        "tagline": "Drafts, edits, tone",
    },
    {
        "id": "artist",
        "name": "Image Muse",
        "avatar": "🎨",
        "color": "#bb6bd9",
        "status": "online",
        "tagline": "Generates visual ideas",
    },
]

BASE_CONVERSATIONS = {
    "assistant": [
        {
            "id": "m1",
            "sender": "agent",
            "type": "text",
            "text": "Hi! I am your General Assistant. How can I help today?",
            "ts": time.time() - 60 * 30,
        }
    ],
    "coder": [
        {
            "id": "m1",
            "sender": "agent",
            "type": "text",
            "text": "Ready to look at some code. Paste an error or ask a question.",
            "ts": time.time() - 60 * 120,
        }
    ],
    "writer": [
        {
            "id": "m1",
            "sender": "agent",
            "type": "text",
            "text": "Send me a draft and I will help tighten it up.",
            "ts": time.time() - 60 * 400,
        }
    ],
    "artist": [
        {
            "id": "m1",
            "sender": "agent",
            "type": "text",
            "text": "Describe a scene and I will sketch a mock preview for you.",
            "ts": time.time() - 60 * 500,
        }
    ],
}

REPLIES = [
    "Got it — let me think about that.",
    "Here is a quick take on it.",
    "Interesting question. Can you share more detail?",
    "I would approach this in a few steps: clarify the goal, gather context, propose an answer.",
    "Sure, here is a code-snippet style reply for testing.",
]


class ChatStore:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._sessions: dict[str, dict[str, list[dict[str, object]]]] = {}
        self._next_id = 1000

    def get_agents(self) -> list[dict[str, str]]:
        return [deepcopy(agent) for agent in AGENTS]

    def get_agent(self, agent_id: str) -> dict[str, str] | None:
        for agent in AGENTS:
            if agent["id"] == agent_id:
                return deepcopy(agent)
        return None

    def get_messages(self, phone: str, agent_id: str) -> list[dict[str, object]]:
        with self._lock:
            conv = self._ensure_conversation(phone, agent_id)
            return [deepcopy(msg) for msg in conv]

    def send_user_message(self, phone: str, agent_id: str, text: str) -> None:
        with self._lock:
            conv = self._ensure_conversation(phone, agent_id)
            conv.append(
                {
                    "id": f"u{self._next_message_id()}",
                    "sender": "user",
                    "type": "text",
                    "text": text,
                    "ts": time.time(),
                }
            )

    def add_agent_reply(self, phone: str, agent_id: str) -> None:
        with self._lock:
            conv = self._ensure_conversation(phone, agent_id)
            conv.append(
                {
                    "id": f"a{self._next_message_id()}",
                    "sender": "agent",
                    "type": "text",
                    "text": random.choice(REPLIES),
                    "ts": time.time(),
                }
            )

    def _ensure_conversation(
        self, phone: str, agent_id: str
    ) -> list[dict[str, object]]:
        sessions = self._sessions.setdefault(phone, {})
        if agent_id not in sessions:
            sessions[agent_id] = [
                deepcopy(msg) for msg in BASE_CONVERSATIONS.get(agent_id, [])
            ]
        return sessions[agent_id]

    def _next_message_id(self) -> int:
        value = self._next_id
        self._next_id += 1
        return value


chat_store = ChatStore()
