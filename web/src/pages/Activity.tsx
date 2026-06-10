import { Link } from 'react-router-dom'
import { useActivity } from '../api/client'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatRelative, formatTime } from '../lib/format'
import { narrative } from '../lib/narrative'

export default function Activity() {
  const query = useActivity()
  return (
    <div>
      <h1 className="text-3xl">Activity</h1>
      <QueryGate query={query}>
        {(items) => (
          <div className="mt-6">
            <Panel>
              {items.length === 0 ? (
                <span className="text-sm text-ink-5">
                  Only one manifest version retained — no transitions to show
                  yet.
                </span>
              ) : (
                <ol className="divide-y divide-ink-7/50">
                  {items.map((item) => (
                    <li key={item.b} className="flex items-baseline gap-4 py-2.5">
                      <span
                        className="w-20 shrink-0 text-right text-xs text-ink-4"
                        title={formatTime(item.at)}
                      >
                        {formatRelative(item.at)}
                      </span>
                      <Link
                        to={`/manifests/diff?a=${item.a}&b=${item.b}`}
                        className="shrink-0 font-mono text-xs text-accent hover:text-accent-high"
                      >
                        #{item.a} → #{item.b}
                      </Link>
                      <span className="min-w-0 text-sm text-ink-2">
                        {narrative(item.diff)}
                      </span>
                    </li>
                  ))}
                </ol>
              )}
              <p className="mt-3 text-xs text-ink-5">
                Newest first; each entry summarizes one manifest transition.
                Click a transition for the full diff.
              </p>
            </Panel>
          </div>
        )}
      </QueryGate>
    </div>
  )
}
