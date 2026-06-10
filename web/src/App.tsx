import { NavLink, Route, Routes } from 'react-router-dom'
import { useHealth } from './api/client'
import { RefreshTimer } from './components/RefreshTimer'
import Overview from './pages/Overview'
import Activity from './pages/Activity'
import Lsm from './pages/Lsm'
import Manifests from './pages/Manifests'
import ManifestDetail from './pages/ManifestDetail'
import ManifestDiff from './pages/ManifestDiff'
import Compactions from './pages/Compactions'
import Checkpoints from './pages/Checkpoints'
import Garbage from './pages/Garbage'
import Wal from './pages/Wal'

const NAV = [
  { to: '/', label: 'Overview' },
  { to: '/activity', label: 'Activity' },
  { to: '/lsm', label: 'LSM Tree' },
  { to: '/wal', label: 'WAL' },
  { to: '/manifests', label: 'Manifests' },
  { to: '/compactions', label: 'Compactions' },
  { to: '/checkpoints', label: 'Checkpoints' },
  { to: '/garbage', label: 'Garbage' },
]

export default function App() {
  const health = useHealth()
  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-20 border-b border-ink-7 bg-surface-0/85 shadow-sm backdrop-blur-md backdrop-saturate-150">
        <div className="mx-auto flex h-14 max-w-7xl items-center gap-6 px-4">
          <a href="/" className="flex items-center">
            <img src="/img/logo-full.svg" alt="SlateDB" className="h-7" />
          </a>
          <nav className="flex items-center gap-1 text-sm font-medium">
            {NAV.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.to === '/'}
                className={({ isActive }) =>
                  `rounded-md px-3 py-1.5 transition-colors ${
                    isActive
                      ? 'bg-accent-low text-accent-high'
                      : 'text-ink-3 hover:bg-surface-2 hover:text-ink-1'
                  }`
                }
              >
                {item.label}
              </NavLink>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-3">
            <RefreshTimer />
            {health.data && (
              <span className="hidden font-mono text-xs text-ink-4 md:inline">
                {health.data.provider}://{health.data.db_path}
              </span>
            )}
            <span className="rounded-full border border-ink-6 bg-surface-2 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-ink-4">
              read-only
            </span>
          </div>
        </div>
      </header>
      <main className="mx-auto max-w-7xl px-4 py-8">
        <Routes>
          <Route path="/" element={<Overview />} />
          <Route path="/activity" element={<Activity />} />
          <Route path="/lsm" element={<Lsm />} />
          <Route path="/wal" element={<Wal />} />
          <Route path="/manifests" element={<Manifests />} />
          <Route path="/manifests/diff" element={<ManifestDiff />} />
          <Route path="/manifests/:id" element={<ManifestDetail />} />
          <Route path="/compactions" element={<Compactions />} />
          <Route path="/checkpoints" element={<Checkpoints />} />
          <Route path="/garbage" element={<Garbage />} />
        </Routes>
      </main>
    </div>
  )
}
