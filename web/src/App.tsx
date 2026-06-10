import { useState } from 'react'
import {
  matchPath,
  NavLink,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router-dom'
import { useDbs, useOverview } from './api/client'
import { RefreshTimer } from './components/RefreshTimer'
import Fleet from './pages/Fleet'
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
  { to: '', label: 'Overview', icon: 'home' },
  { to: '/alerts', label: 'Alerts', icon: 'bell' },
  { to: '/activity', label: 'Activity', icon: 'pulse' },
  { to: '/lsm', label: 'LSM Tree', icon: 'layers' },
  { to: '/wal', label: 'WAL', icon: 'log' },
  { to: '/manifests', label: 'Manifests', icon: 'files' },
  { to: '/compactions', label: 'Compactions', icon: 'funnel' },
  { to: '/checkpoints', label: 'Checkpoints', icon: 'flag' },
  { to: '/garbage', label: 'Garbage', icon: 'trash' },
] as const

type IconName = (typeof NAV)[number]['icon'] | 'grid'

const ICON_PATHS: Record<IconName, string> = {
  home: 'M3 9l7-6 7 6v8h-4.5v-5h-5v5H3V9z',
  bell: 'M15.5 13.5H4.5c1-1 1.5-2.2 1.5-5a4 4 0 1 1 8 0c0 2.8.5 4 1.5 5zM8.5 16a1.5 1.5 0 0 0 3 0',
  pulse: 'M2 10.5h3.5L8 4l4 12 2.5-5.5H18',
  layers: 'M10 2.5l7.5 3.75L10 10 2.5 6.25 10 2.5zM2.5 10 10 13.75 17.5 10M2.5 13.75 10 17.5l7.5-3.75',
  log: 'M5.5 2.5h6l3 3v12h-9v-15zM11.5 2.5v3h3M8 9.5h4M8 12.5h4',
  files: 'M7 6.5h9.5V18H7V6.5zM4 13.5V2.5h9',
  funnel: 'M3 3.5h14l-5.5 6.5v5l-3 2v-7L3 3.5z',
  flag: 'M5 17.5v-15M5 3.5h9.5l-2 3 2 3H5',
  trash: 'M3.5 5.5h13M8 5.5v-2h4v2M5.5 5.5l1 12h7l1-12M8.5 8.5v6M11.5 8.5v6',
  grid: 'M3 3h6v6H3V3zM11 3h6v6h-6V3zM3 11h6v6H3v-6zM11 11h6v6h-6v-6z',
}

function NavIcon({ name }: { name: IconName }) {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="shrink-0"
      aria-hidden
    >
      <path d={ICON_PATHS[name]} />
    </svg>
  )
}

const NAV_LINK_STYLE = ({ isActive }: { isActive: boolean }) =>
  `flex items-center gap-2.5 rounded-md px-3 py-2 transition-colors ${
    isActive
      ? 'bg-accent-low text-accent-high'
      : 'text-ink-3 hover:bg-surface-2 hover:text-ink-1'
  }`

/**
 * Sidebar contents: the fleet link, and — when a DB is active — the DB
 * switcher plus that DB's page nav.
 */
function SidebarNav({
  dbId,
  subPath,
  alertCount,
  onNavigate,
}: {
  dbId: string | null
  subPath: string
  alertCount: number
  onNavigate?: () => void
}) {
  const dbs = useDbs()
  const navigate = useNavigate()
  const list = dbs.data?.dbs ?? []
  return (
    <nav className="flex flex-col gap-0.5 text-sm font-medium">
      <NavLink to="/" end onClick={onNavigate} className={NAV_LINK_STYLE}>
        <NavIcon name="grid" />
        <span className="flex-1">All databases</span>
        {list.length > 0 && (
          <span className="text-xs text-ink-5">{list.length}</span>
        )}
      </NavLink>
      {dbId !== null && (
        <>
          {list.length > 1 && (
            <select
              value={dbId}
              onChange={(e) => {
                navigate(`/db/${encodeURIComponent(e.target.value)}${subPath}`)
                onNavigate?.()
              }}
              className="mt-2 w-full rounded-md border border-ink-6 bg-surface-1 px-2 py-1.5 text-sm text-ink-2"
              aria-label="Switch database"
            >
              {list.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.path} ({d.store})
                </option>
              ))}
            </select>
          )}
          <div className="mt-2 flex flex-col gap-0.5 border-t border-ink-7/60 pt-2">
            {NAV.map((item) => (
              <NavLink
                key={item.to}
                to={`/db/${encodeURIComponent(dbId)}${item.to}`}
                end={item.to === ''}
                onClick={onNavigate}
                className={NAV_LINK_STYLE}
              >
                <NavIcon name={item.icon} />
                <span className="flex-1">{item.label}</span>
                {item.to === '/alerts' && alertCount > 0 && (
                  <span className="rounded-full bg-accent px-1.5 py-0.5 text-xs font-semibold leading-none text-white">
                    {alertCount}
                  </span>
                )}
              </NavLink>
            ))}
          </div>
        </>
      )}
    </nav>
  )
}

export default function App() {
  const location = useLocation()
  const [navOpen, setNavOpen] = useState(false)
  const match = matchPath('/db/:dbId/*', location.pathname)
  const dbId = match?.params.dbId ? decodeURIComponent(match.params.dbId) : null
  const subPath = match?.params['*'] ? `/${match.params['*']}` : ''
  // Info-level notes don't warrant a badge; warn/error do. Disabled on the
  // fleet page (no active DB).
  const overview = useOverview(dbId ?? undefined)
  const alertCount =
    dbId === null
      ? 0
      : (overview.data?.warnings.filter((w) => w.severity !== 'info').length ?? 0)
  return (
    <div className="min-h-screen lg:pl-56">
      {/* Desktop: full-height drawer flush against the left edge. */}
      <aside className="fixed inset-y-0 left-0 z-30 hidden w-56 flex-col border-r border-ink-7 bg-surface-1 lg:flex">
        <div className="flex h-14 shrink-0 items-center border-b border-ink-7 px-4">
          <a href="/" className="flex items-center">
            <img src="/img/logo-full.svg" alt="SlateDB" className="h-7" />
          </a>
        </div>
        <div className="flex-1 overflow-y-auto p-3">
          <SidebarNav dbId={dbId} subPath={subPath} alertCount={alertCount} />
        </div>
        {dbId && (
          <div className="shrink-0 border-t border-ink-7 px-4 py-3 font-mono text-xs text-ink-4">
            {dbId}
          </div>
        )}
      </aside>

      {/* Styled identically to the sidebar's logo row — h-14 with the
          border INSIDE the box, surface-1, ink-7 — so the two read as one
          continuous, aligned bar. */}
      <header className="sticky top-0 z-20">
        <div className="flex h-14 items-center gap-3 border-b border-ink-7 bg-surface-1 px-4">
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
          <a href="/" className="flex items-center lg:hidden">
            <img src="/img/logo-full.svg" alt="SlateDB" className="h-7" />
          </a>
          <div className="ml-auto flex items-center gap-3">
            {dbId && (
              <span className="hidden font-mono text-xs text-ink-4 md:inline lg:hidden">
                {dbId}
              </span>
            )}
            <span className="rounded-full border border-ink-6 bg-surface-2 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-ink-4">
              read-only
            </span>
            <RefreshTimer />
          </div>
        </div>
      </header>

      {/* Mobile: hamburger-toggled overlay drawer. */}
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
            <SidebarNav
              dbId={dbId}
              subPath={subPath}
              alertCount={alertCount}
              onNavigate={() => setNavOpen(false)}
            />
          </aside>
        </div>
      )}

      <main className="mx-auto min-w-0 max-w-6xl px-4 py-8">
        <Routes>
          <Route path="/" element={<Fleet />} />
          <Route path="/db/:dbId">
            <Route index element={<Overview />} />
            <Route path="alerts" element={<Alerts />} />
            <Route path="activity" element={<Activity />} />
            <Route path="lsm" element={<Lsm />} />
            <Route path="wal" element={<Wal />} />
            <Route path="manifests" element={<Manifests />} />
            <Route path="manifests/diff" element={<ManifestDiff />} />
            <Route path="manifests/:id" element={<ManifestDetail />} />
            <Route path="compactions" element={<Compactions />} />
            <Route path="checkpoints" element={<Checkpoints />} />
            <Route path="garbage" element={<Garbage />} />
          </Route>
        </Routes>
      </main>
    </div>
  )
}
