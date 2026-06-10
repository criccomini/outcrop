import { useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useDbPath, useManifests } from '../api/client'
import { HelpTip } from '../components/HelpTip'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatBytes, formatRelative } from '../lib/format'

export default function Manifests() {
  const query = useManifests()
  const navigate = useNavigate()
  const dbPath = useDbPath()
  // In pick order, capped at two: a third pick replaces the oldest one.
  const [picked, setPicked] = useState<number[]>([])
  // The diff itself is always oldest → newest.
  const [a, b] = [...picked].sort((x, y) => x - y)

  function toggle(id: number) {
    setPicked((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev.slice(-1), id],
    )
  }

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-3xl">Manifests</h1>
        <button
          disabled={picked.length !== 2}
          onClick={() => navigate(dbPath(`/manifests/diff?a=${a}&b=${b}`))}
          className="rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-accent-high disabled:cursor-not-allowed disabled:bg-ink-6"
        >
          Diff {picked.length === 2 ? `#${a} → #${b}` : 'two versions'}
        </button>
      </div>
      <div className="mt-6">
        <QueryGate query={query}>
          {(manifests) => (
            <Panel
              action={
                <HelpTip>
                  Newest first. Pick two versions with the checkboxes, then
                  use the Diff button to compare them.
                </HelpTip>
              }
            >
              <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                    <th className="pb-2 pr-2"></th>
                    <th className="pb-2 pr-4">ID</th>
                    <th className="pb-2 pr-4">Written</th>
                    <th className="pb-2 pr-4">L0</th>
                    <th className="pb-2 pr-4">Runs</th>
                    <th className="pb-2 pr-4">SSTs</th>
                    <th className="pb-2 pr-4">Size</th>
                    <th className="pb-2 pr-4">Checkpoints</th>
                    <th className="pb-2">Epochs (w/c)</th>
                  </tr>
                </thead>
                <tbody>
                  {manifests.map((m) => (
                    <tr
                      key={m.id}
                      className="border-t border-ink-7/50 hover:bg-surface-2"
                    >
                      <td className="py-1.5 pr-2">
                        <input
                          type="checkbox"
                          checked={picked.includes(m.id)}
                          onChange={() => toggle(m.id)}
                          className="accent-[#b26844]"
                        />
                      </td>
                      <td className="py-1.5 pr-4">
                        <Link
                          to={dbPath(`/manifests/${m.id}`)}
                          className="font-mono text-accent hover:text-accent-high"
                        >
                          #{m.id}
                        </Link>
                      </td>
                      <td
                        className="py-1.5 pr-4 text-ink-3"
                        title={m.last_modified}
                      >
                        {formatRelative(m.last_modified)}
                      </td>
                      <td className="py-1.5 pr-4">{m.l0_count}</td>
                      <td className="py-1.5 pr-4">{m.sorted_run_count}</td>
                      <td className="py-1.5 pr-4">{m.sst_count}</td>
                      <td className="py-1.5 pr-4">
                        {formatBytes(m.est_total_bytes)}
                      </td>
                      <td className="py-1.5 pr-4">{m.checkpoint_count}</td>
                      <td className="py-1.5 font-mono text-xs text-ink-4">
                        {m.writer_epoch}/{m.compactor_epoch}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              </div>
            </Panel>
          )}
        </QueryGate>
      </div>
    </div>
  )
}
