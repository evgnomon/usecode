import { useEffect, useRef, useState } from 'react'
import { fetchMessages, sendMessage, requestAgentReply } from '../api/mockApi'
import MessageBubble from './MessageBubble'
import Composer from './Composer'

export default function ChatView({ agent, onBack }) {
  const [messages, setMessages] = useState([])
  const [loading, setLoading] = useState(true)
  const [agentTyping, setAgentTyping] = useState(false)
  const bottomRef = useRef(null)

  useEffect(() => {
    let active = true
    setLoading(true)
    fetchMessages(agent.id).then((msgs) => {
      if (active) {
        setMessages(msgs)
        setLoading(false)
      }
    })
    return () => {
      active = false
    }
  }, [agent.id])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, agentTyping])

  function handleSend(text) {
    sendMessage(agent.id, text).then((userMessage) => {
      setMessages((prev) => [...prev, userMessage])
      setAgentTyping(true)
      requestAgentReply(agent.id).then((reply) => {
        setAgentTyping(false)
        setMessages((prev) => [...prev, reply])
      })
    })
  }

  return (
    <div className="chat-view">
      <header className="chat-header">
        <button type="button" className="back-button" onClick={onBack} aria-label="Back to agents">
          ‹
        </button>
        <span className="avatar" style={{ background: agent.color }}>
          {agent.avatar}
          <span className={`status-dot ${agent.status}`} />
        </span>
        <div className="chat-header-meta">
          <span className="agent-name">{agent.name}</span>
          <span className="agent-status-text">{agentTyping ? 'typing…' : agent.status}</span>
        </div>
      </header>

      <div className="chat-messages">
        {loading ? (
          <div className="chat-loading">Loading conversation…</div>
        ) : (
          <>
            {messages.map((m) => (
              <MessageBubble key={m.id} message={m} />
            ))}
            {agentTyping && (
              <div className="bubble-row">
                <div className="bubble agent typing">
                  <span className="dot" />
                  <span className="dot" />
                  <span className="dot" />
                </div>
              </div>
            )}
          </>
        )}
        <div ref={bottomRef} />
      </div>

      <Composer onSend={handleSend} disabled={loading} />
    </div>
  )
}
