import { useCallback, useEffect, useState } from 'react'
import { fetchMe, logout as logoutRequest } from '../api/authApi'

const STORAGE_KEY = 'usecode-agent.apiKey'

export function useAuth() {
  const [apiKey, setApiKey] = useState(() => localStorage.getItem(STORAGE_KEY))
  const [phone, setPhone] = useState(null)
  const [checking, setChecking] = useState(true)

  useEffect(() => {
    if (!apiKey) {
      setChecking(false)
      return
    }
    fetchMe(apiKey)
      .then((me) => setPhone(me.phone))
      .catch(() => {
        localStorage.removeItem(STORAGE_KEY)
        setApiKey(null)
      })
      .finally(() => setChecking(false))
  }, [apiKey])

  const login = useCallback((newApiKey, newPhone) => {
    localStorage.setItem(STORAGE_KEY, newApiKey)
    setApiKey(newApiKey)
    setPhone(newPhone)
  }, [])

  const logout = useCallback(() => {
    if (apiKey) logoutRequest(apiKey).catch(() => {})
    localStorage.removeItem(STORAGE_KEY)
    setApiKey(null)
    setPhone(null)
  }, [apiKey])

  return { apiKey, phone, checking, isAuthenticated: Boolean(apiKey && phone), login, logout }
}
