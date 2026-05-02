import { useEffect, useState } from 'react'
import type { Agent24API } from '../main/preload'

declare global {
  interface Window {
    agent24: Agent24API
  }
}

export function App(): JSX.Element {
  const [version, setVersion] = useState<string>('—')
  const [pingResult, setPingResult] = useState<string>('—')

  useEffect(() => {
    void window.agent24.getAppVersion().then(setVersion)
  }, [])

  const handlePing = async (): Promise<void> => {
    const result = await window.agent24.ping()
    setPingResult(result)
  }

  return (
    <div className="app">
      <header>
        <h1>Agent24</h1>
        <p className="subtitle">Pluggable cross-platform agent framework</p>
      </header>
      <main>
        <section className="status">
          <div>
            <span className="label">Version:</span> <code>{version}</code>
          </div>
          <div>
            <span className="label">IPC:</span> <code>{pingResult}</code>
            <button onClick={handlePing}>ping</button>
          </div>
        </section>
        <section className="placeholder">
          <p>M1 scaffolding. Capability modules and full UI arrive in M1+.</p>
          <p className="hint">
            See{' '}
            <a
              href="#"
              onClick={(e) => {
                e.preventDefault()
                void window.agent24.openExternal(
                  'https://github.com/AuraAIHQ/Agent24-Desktop/blob/main/docs/ROADMAP.md',
                )
              }}
            >
              ROADMAP
            </a>{' '}
            for next milestones.
          </p>
        </section>
      </main>
    </div>
  )
}
