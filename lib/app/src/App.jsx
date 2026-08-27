import { useEffect, useState } from 'react'
import { fetchAgents, fetchMessages } from './api/mockApi'
import AgentList from './components/AgentList'
import ChatView from './components/ChatView'
import LoginView from './components/LoginView'
import { useAuth } from './hooks/useAuth'
import './App.css'

export default function App() {
  const auth = useAuth()
  const [agents, setAgents] = useState([])
  const [activeAgentId, setActiveAgentId] = useState(null)
  const [previews, setPreviews] = useState({})
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!auth.isAuthenticated) return
    fetchAgents().then((list) => {
      setAgents(list)
      setLoading(false)
      list.forEach((agent) => {
        fetchMessages(agent.id).then((msgs) => {
          if (msgs.length) {
            setPreviews((prev) => ({ ...prev, [agent.id]: msgs[msgs.length - 1] }))
          }
        })
      })
    })
  }, [auth.isAuthenticated])

  const activeAgent = agents.find((a) => a.id === activeAgentId) || null

  if (auth.checking) {
    return <div className="chat-loading">Loading…</div>
  }

  if (!auth.isAuthenticated) {
    return <LoginView onLogin={auth.login} />
  }

  return (
    <div className={`app-shell${activeAgent ? ' chat-open' : ''}`}>
      <aside className="sidebar">
        <header className="sidebar-header">
          <h1>usecode agent</h1>
          <button className="logout-button" onClick={auth.logout}>
            Log out
          </button>
        </header>
        {loading ? (
          <div className="chat-loading">Loading agents…</div>
        ) : (
          <AgentList
            agents={agents}
            activeAgentId={activeAgentId}
            onSelect={setActiveAgentId}
            previews={previews}
          />
        )}
      </aside>

      <main className="main-panel">
        {activeAgent ? (
          <ChatView agent={activeAgent} onBack={() => setActiveAgentId(null)} />
        ) : (
          <div className="empty-state">
            <p>Select an agent to start chatting</p>
          </div>
        )}
      </main>
    </div>
  )
}
