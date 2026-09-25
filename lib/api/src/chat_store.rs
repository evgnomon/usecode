// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Mock chat conversations for the web UI, kept in memory per process.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::IndexedRandom;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub avatar: &'static str,
    pub color: &'static str,
    pub status: &'static str,
    pub tagline: &'static str,
}

pub const AGENTS: &[Agent] = &[
    Agent {
        id: "assistant",
        name: "General Assistant",
        avatar: "🤖",
        color: "#5b8def",
        status: "online",
        tagline: "Ask me anything",
    },
    Agent {
        id: "coder",
        name: "Code Helper",
        avatar: "💻",
        color: "#3ecf8e",
        status: "online",
        tagline: "Debugging & code review",
    },
    Agent {
        id: "writer",
        name: "Writing Coach",
        avatar: "✍️",
        color: "#f2994a",
        status: "away",
        tagline: "Drafts, edits, tone",
    },
    Agent {
        id: "artist",
        name: "Image Muse",
        avatar: "🎨",
        color: "#bb6bd9",
        status: "online",
        tagline: "Generates visual ideas",
    },
];

// (agent id, greeting, minutes before process start)
const GREETINGS: &[(&str, &str, f64)] = &[
    (
        "assistant",
        "Hi! I am your General Assistant. How can I help today?",
        30.0,
    ),
    (
        "coder",
        "Ready to look at some code. Paste an error or ask a question.",
        120.0,
    ),
    (
        "writer",
        "Send me a draft and I will help tighten it up.",
        400.0,
    ),
    (
        "artist",
        "Describe a scene and I will sketch a mock preview for you.",
        500.0,
    ),
];

const REPLIES: &[&str] = &[
    "Got it — let me think about that.",
    "Here is a quick take on it.",
    "Interesting question. Can you share more detail?",
    "I would approach this in a few steps: clarify the goal, gather context, propose an answer.",
    "Sure, here is a code-snippet style reply for testing.",
];

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub sender: &'static str,
    pub r#type: &'static str,
    pub text: String,
    pub ts: f64,
}

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

struct State {
    // phone -> agent id -> conversation
    sessions: HashMap<String, HashMap<String, Vec<Message>>>,
    next_id: u64,
}

pub struct ChatStore {
    started: f64,
    state: Mutex<State>,
}

impl Default for ChatStore {
    fn default() -> Self {
        Self {
            started: now(),
            state: Mutex::new(State {
                sessions: HashMap::new(),
                next_id: 1000,
            }),
        }
    }
}

impl ChatStore {
    pub fn get_agent(&self, agent_id: &str) -> Option<&'static Agent> {
        AGENTS.iter().find(|agent| agent.id == agent_id)
    }

    fn greeting(&self, agent_id: &str) -> Vec<Message> {
        GREETINGS
            .iter()
            .filter(|(id, _, _)| *id == agent_id)
            .map(|(_, text, minutes_ago)| Message {
                id: "m1".to_string(),
                sender: "agent",
                r#type: "text",
                text: text.to_string(),
                ts: self.started - 60.0 * minutes_ago,
            })
            .collect()
    }

    fn with_conversation<T>(
        &self,
        phone: &str,
        agent_id: &str,
        f: impl FnOnce(&mut Vec<Message>, &mut u64) -> T,
    ) -> T {
        let mut state = self.state.lock().unwrap();
        let State { sessions, next_id } = &mut *state;
        let conversation = sessions
            .entry(phone.to_string())
            .or_default()
            .entry(agent_id.to_string())
            .or_insert_with(|| self.greeting(agent_id));
        f(conversation, next_id)
    }

    pub fn get_messages(&self, phone: &str, agent_id: &str) -> Vec<Message> {
        self.with_conversation(phone, agent_id, |conversation, _| conversation.clone())
    }

    pub fn send_user_message(&self, phone: &str, agent_id: &str, text: &str) {
        self.with_conversation(phone, agent_id, |conversation, next_id| {
            conversation.push(Message {
                id: format!("u{next_id}"),
                sender: "user",
                r#type: "text",
                text: text.to_string(),
                ts: now(),
            });
            *next_id += 1;
        });
    }

    pub fn add_agent_reply(&self, phone: &str, agent_id: &str) {
        let reply = REPLIES
            .choose(&mut rand::rng())
            .copied()
            .unwrap_or_default();
        self.with_conversation(phone, agent_id, |conversation, next_id| {
            conversation.push(Message {
                id: format!("a{next_id}"),
                sender: "agent",
                r#type: "text",
                text: reply.to_string(),
                ts: now(),
            });
            *next_id += 1;
        });
    }
}
