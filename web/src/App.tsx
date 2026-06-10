import { useState } from 'react'
import { NavLink, Route, Routes } from 'react-router-dom'
import { useHealth, useOverview } from './api/client'
import { RefreshTimer } from './components/RefreshTimer'
import Overview from './pages/Overview'
import Alerts from './pages/Alerts'
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
  { to: '/alerts', label: 'Alerts' },
  { to: '/activity', label: 'Activity' },
  { to: '/lsm', label: 'LSM Tree' },
  { to: '/wal', label: 'WAL' },
  { to: '/manifests', label: 'Manifests' },
  { to: '/compactions', label: 'Compactions' },
  { to: '/checkpoints', label: 'Checkpoints' },
  { to: '/garbage', label: 'Garbage' },
]

function NavList({
  alertCount,
  onNavigate,
}: {
  alertCount: number
  onNavigate?: () => void
}) {
  return (
    <nav className="flex flex-col gap-1 text-sm font-medium">
      {NAV.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          end={item.to === '/'}
          onClick={onNavigate}
          className={({ isActive }) =>
            `flex items-center justify-between rounded-md px-3 py-1.5 transition-colors ${
              isActive
                ? 'bg-accent-low text-accent-high'
                : 'text-ink-3 hover:bg-surface-2 hover:text-ink-1'
            }`
          }
        >
          {item.label}
          {item.to === '/alerts' && alertCount > 0 && (
            <span className="ml-2 rounded-full bg-accent px-1.5 py-0.5 text-xs font-semibold leading-none text-white">
              {alertCount}
            </span>
          )}
        </NavLink>
      ))}
    </nav>
  )
}

export default function App() {
  const health = useHealth()
  const overview = useOverview()
  const [navOpen, setNavOpen] = useState(false)
  // Info-level notes don't warrant a badge; warn/error do.
  const alertCount =
    overview.data?.warnings.filter((w) => w.severity !== 'info').length ?? 0
  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-20 border-b border-ink-7 bg-surface-0/85 shadow-sm backdrop-blur-md backdrop-saturate-150">
        <div className="mx-auto flex h-14 max-w-7xl items-center gap-3 px-4">
          <button
            onClick={() => setNavOpen(true)}
            className="rounded-md p-1.5 text-ink-3 hover:bg-surface-2 hover:text-ink-1 lg:hidden"
            aria-label="Open navigation"
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 20 20"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            >
              <path d="M3 5h14M3 10h14M3 15h14" />
            </svg>
          </button>
          <a href="/" className="flex items-center">
            <img src="/img/logo-full.svg" alt="SlateDB" className="h-7" />
          </a>
          <div className="ml-auto flex items-center gap-3">
            {health.data && (
              <span className="hidden font-mono text-xs text-ink-4 md:inline">
                {health.data.provider}://{health.data.db_path}
              </span>
            )}
            <span className="rounded-full border border-ink-6 bg-surface-2 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-ink-4">
              read-only
            </span>
            <RefreshTimer />
          </div>
        </div>
      </header>
      {navOpen && (
        <div className="fixed inset-0 z-40 lg:hidden">
          <div
            className="absolute inset-0 bg-ink-1/30"
            onClick={() => setNavOpen(false)}
            aria-hidden
          />
          <aside className="absolute inset-y-0 left-0 w-60 overflow-y-auto border-r border-ink-7 bg-surface-1 p-4 shadow-lg">
            <div className="mb-3 flex items-center justify-between">
              <img src="/img/logo-full.svg" alt="SlateDB" className="h-6" />
              <button
                onClick={() => setNavOpen(false)}
                className="rounded-md px-2 py-1 text-ink-4 hover:bg-surface-2 hover:text-ink-1"
                aria-label="Close navigation"
              >
                ✕
              </button>
            </div>
            <NavList alertCount={alertCount} onNavigate={() => setNavOpen(false)} />
          </aside>
        </div>
      )}
      <div className="mx-auto flex max-w-7xl items-start">
        <aside className="sticky top-14 hidden max-h-[calc(100vh-3.5rem)] w-44 shrink-0 overflow-y-auto py-8 pl-4 lg:block">
          <NavList alertCount={alertCount} />
        </aside>
        <main className="min-w-0 flex-1 px-4 py-8">
          <Routes>
            <Route path="/" element={<Overview />} />
            <Route path="/alerts" element={<Alerts />} />
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
    </div>
  )
}
