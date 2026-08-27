// Mock API layer — swap these with real fetch() calls to your backend later.
// Every function returns a Promise to mirror real network calls.

const NETWORK_DELAY = 400

function delay(ms = NETWORK_DELAY) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

const AGENTS = [
  {
    id: 'assistant',
    name: 'General Assistant',
    avatar: '🤖',
    color: '#5b8def',
    status: 'online',
    tagline: 'Ask me anything',
  },
  {
    id: 'coder',
    name: 'Code Helper',
    avatar: '💻',
    color: '#3ecf8e',
    status: 'online',
    tagline: 'Debugging & code review',
  },
  {
    id: 'writer',
    name: 'Writing Coach',
    avatar: '✍️',
    color: '#f2994a',
    status: 'away',
    tagline: 'Drafts, edits, tone',
  },
  {
    id: 'artist',
    name: 'Image Muse',
    avatar: '🎨',
    color: '#bb6bd9',
    status: 'online',
    tagline: 'Generates visual ideas',
  },
]

const CONVERSATIONS = {
  assistant: [
    {
      id: 'm1',
      sender: 'agent',
      type: 'text',
      text: 'Hi! I am your *General Assistant*. How can I help today?',
      ts: Date.now() - 1000 * 60 * 30,
    },
  ],
  coder: [
    {
      id: 'm1',
      sender: 'agent',
      type: 'text',
      text: 'Ready to look at some `code`. Paste an error or ask a question.',
      ts: Date.now() - 1000 * 60 * 120,
    },
  ],
  writer: [
    {
      id: 'm1',
      sender: 'agent',
      type: 'text',
      text: 'Send me a draft and I will help tighten it up.',
      ts: Date.now() - 1000 * 60 * 400,
    },
  ],
  artist: [
    {
      id: 'm1',
      sender: 'agent',
      type: 'text',
      text: 'Describe a scene and I will sketch a *mock* preview for you.',
      ts: Date.now() - 1000 * 60 * 500,
    },
    {
      id: 'm2',
      sender: 'agent',
      type: 'image',
      imageUrl: 'https://picsum.photos/seed/usecode-agent/480/320',
      caption: 'A placeholder preview image',
      ts: Date.now() - 1000 * 60 * 499,
    },
  ],
}

const REPLIES = [
  'Got it — let me think about that.',
  'Here is a *quick* take on it.',
  'Interesting question. Can you share more detail?',
  'I would approach this in a few steps:\n1. Clarify the goal\n2. Gather context\n3. Propose an answer',
  'Sure, here is a `code snippet` style reply for testing.',
]

let nextId = 1000

export function fetchAgents() {
  return delay().then(() => AGENTS.map((a) => ({ ...a })))
}

export function fetchMessages(agentId) {
  return delay().then(() => (CONVERSATIONS[agentId] || []).map((m) => ({ ...m })))
}

export function sendMessage(agentId, text) {
  const userMessage = {
    id: `u${nextId++}`,
    sender: 'user',
    type: 'text',
    text,
    ts: Date.now(),
  }
  if (!CONVERSATIONS[agentId]) CONVERSATIONS[agentId] = []
  CONVERSATIONS[agentId].push(userMessage)

  return delay(150).then(() => userMessage)
}

export function requestAgentReply(agentId) {
  const reply = {
    id: `a${nextId++}`,
    sender: 'agent',
    type: 'text',
    text: REPLIES[Math.floor(Math.random() * REPLIES.length)],
    ts: Date.now(),
  }
  return delay(700 + Math.random() * 600).then(() => {
    if (!CONVERSATIONS[agentId]) CONVERSATIONS[agentId] = []
    CONVERSATIONS[agentId].push(reply)
    return reply
  })
}
