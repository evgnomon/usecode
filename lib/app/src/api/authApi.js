const API_BASE = import.meta.env.VITE_API_URL || '/api'

async function parseResponse(res) {
  const data = await res.json().catch(() => null)
  if (!res.ok) {
    const message = data?.detail || `Request failed (${res.status})`
    throw new Error(typeof message === 'string' ? message : 'Request failed')
  }
  return data
}

export function requestOtp(phone) {
  return fetch(`${API_BASE}/auth/otp/request`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ phone }),
  }).then(parseResponse)
}

export function verifyOtp(phone, code) {
  return fetch(`${API_BASE}/auth/otp/verify`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ phone, code }),
  }).then(parseResponse)
}

export function fetchMe(apiKey) {
  return fetch(`${API_BASE}/auth/me`, {
    headers: { 'X-API-Key': apiKey },
  }).then(parseResponse)
}

export function logout(apiKey) {
  return fetch(`${API_BASE}/auth/logout`, {
    method: 'POST',
    headers: { 'X-API-Key': apiKey },
  })
}
