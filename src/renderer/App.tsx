import { useEffect, useState } from 'react'
import type { Agent24API } from '../main/preload'

declare global {
  interface Window {
    agent24: Agent24API
  }
}

interface ModuleInfo {
  id: string
  state: string
}

const STATE_BADGE_STYLE: Record<string, React.CSSProperties> = {
  loaded:     { background: '#22c55e', color: '#fff', padding: '1px 6px', borderRadius: 4 },
  loading:    { background: '#f59e0b', color: '#fff', padding: '1px 6px', borderRadius: 4 },
  failed:     { background: '#ef4444', color: '#fff', padding: '1px 6px', borderRadius: 4 },
  registered: { background: '#6366f1', color: '#fff', padding: '1px 6px', borderRadius: 4 },
  unloaded:   { background: '#94a3b8', color: '#fff', padding: '1px 6px', borderRadius: 4 },
}

function StateBadge({ state }: { state: string }): JSX.Element {
  const style = STATE_BADGE_STYLE[state] ?? { background: '#e2e8f0', padding: '1px 6px', borderRadius: 4 }
  return <span style={style}>{state}</span>
}

export function App(): JSX.Element {
  const [version, setVersion] = useState<string>('—')
  const [pingResult, setPingResult] = useState<string>('—')
  const [modules, setModules] = useState<ModuleInfo[]>([])

  useEffect(() => {
    void window.agent24.getAppVersion().then(setVersion)
    void window.agent24.kernel.listModules().then(setModules)
  }, [])

  const handlePing = async (): Promise<void> => {
    const result = await window.agent24.ping()
    setPingResult(result)
  }

  const handleRefreshModules = async (): Promise<void> => {
    const result = await window.agent24.kernel.listModules()
    setModules(result)
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
        <section className="modules">
          <h2 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>
            Modules
            <button
              onClick={handleRefreshModules}
              style={{ marginLeft: '0.5rem', fontSize: '0.75rem' }}
            >
              refresh
            </button>
          </h2>
          {modules.length === 0 ? (
            <p style={{ color: '#94a3b8', fontSize: '0.875rem' }}>
              No modules registered yet.
            </p>
          ) : (
            <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
              {modules.map((m) => (
                <li
                  key={m.id}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '0.5rem',
                    padding: '0.25rem 0',
                    fontFamily: 'monospace',
                    fontSize: '0.875rem',
                  }}
                >
                  <code>{m.id}</code>
                  <StateBadge state={m.state} />
                </li>
              ))}
            </ul>
          )}
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
