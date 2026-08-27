import { useState } from 'react'
import { requestOtp, verifyOtp } from '../api/authApi'

export default function LoginView({ onLogin }) {
  const [step, setStep] = useState('phone')
  const [phone, setPhone] = useState('')
  const [code, setCode] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [debugCode, setDebugCode] = useState(null)

  function handleRequestOtp(e) {
    e.preventDefault()
    setError('')
    setLoading(true)
    requestOtp(phone)
      .then((res) => {
        setDebugCode(res.debug_code || null)
        setStep('code')
      })
      .catch((err) => setError(err.message))
      .finally(() => setLoading(false))
  }

  function handleVerifyOtp(e) {
    e.preventDefault()
    setError('')
    setLoading(true)
    verifyOtp(phone, code)
      .then((res) => onLogin(res.api_key, res.phone))
      .catch((err) => setError(err.message))
      .finally(() => setLoading(false))
  }

  return (
    <div className="login-screen">
      <div className="login-card">
        <h1>usecode agent</h1>
        <p className="login-subtitle">
          {step === 'phone' ? 'Sign in with your phone number' : `Enter the code sent to ${phone}`}
        </p>

        {step === 'phone' ? (
          <form onSubmit={handleRequestOtp} className="login-form">
            <input
              type="tel"
              inputMode="tel"
              placeholder="+14155552671"
              value={phone}
              onChange={(e) => setPhone(e.target.value)}
              autoFocus
              required
            />
            {error && <p className="login-error">{error}</p>}
            <button type="submit" disabled={loading || !phone}>
              {loading ? 'Sending…' : 'Send code'}
            </button>
          </form>
        ) : (
          <form onSubmit={handleVerifyOtp} className="login-form">
            <input
              type="text"
              inputMode="numeric"
              pattern="[0-9]*"
              placeholder="6-digit code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              autoFocus
              required
            />
            {debugCode && <p className="login-hint">Dev mode — code: {debugCode}</p>}
            {error && <p className="login-error">{error}</p>}
            <button type="submit" disabled={loading || !code}>
              {loading ? 'Verifying…' : 'Verify'}
            </button>
            <button
              type="button"
              className="login-link"
              onClick={() => {
                setStep('phone')
                setCode('')
                setError('')
              }}
            >
              Use a different number
            </button>
          </form>
        )}
      </div>
    </div>
  )
}
