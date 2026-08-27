function timeAgo(ts) {
  const diffMin = Math.round((Date.now() - ts) / 60000)
  if (diffMin < 1) return 'now'
  if (diffMin < 60) return `${diffMin}m`
  const diffHr = Math.round(diffMin / 60)
  if (diffHr < 24) return `${diffHr}h`
  return `${Math.round(diffHr / 24)}d`
}

export default function AgentList({ agents, activeAgentId, onSelect, previews }) {
  return (
    <ul className="agent-list">
      {agents.map((agent) => {
        const preview = previews[agent.id]
        return (
          <li key={agent.id}>
            <button
              type="button"
              className={`agent-list-item${agent.id === activeAgentId ? ' active' : ''}`}
              onClick={() => onSelect(agent.id)}
            >
              <span className="avatar" style={{ background: agent.color }}>
                {agent.avatar}
                <span className={`status-dot ${agent.status}`} />
              </span>
              <span className="agent-list-meta">
                <span className="agent-list-row">
                  <span className="agent-name">{agent.name}</span>
                  {preview && <span className="agent-time">{timeAgo(preview.ts)}</span>}
                </span>
                <span className="agent-preview">
                  {preview
                    ? preview.type === 'image'
                      ? '📷 Photo'
                      : preview.text
                    : agent.tagline}
                </span>
              </span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}
