# slatedb-dashboard

A **read-only** web dashboard for inspecting a [SlateDB](https://slatedb.io)
database directly from object storage.

SlateDB keeps all of its state — manifests, WAL SSTs, L0 SSTs, sorted runs,
checkpoints — in the object store, so the dashboard needs no cooperation from
the running writer. It performs **zero writes**: only manifest reads, SST
metadata reads, and object listings.

## Features

- **Overview** — health summary: sizes, SST counts, manifest freshness, WAL
  window, epochs, plus a storage & garbage panel (space amplification, bytes
  pinned by checkpoints, and what the GC would reclaim).
- **Alerts** — health warnings (L0 backlog, WAL window growth, stale
  manifests, expired checkpoints) in one place, with a count badge in the
  nav.
- **LSM Tree** — visual tree: levels by size and key-range coverage
  (overlap reads as read amplification), with per-SST drill-down into block
  index, bloom filter/stats sizes, and content stats.
- **Manifests** — full history, structured view of any version, and a
  semantic diff between any two versions ("3 L0 SSTs compacted into SR 7").
- **Compactions** — current compactor state plus history of `.compactions`
  versions.
- **Checkpoints** — checkpoint table with expiry countdowns, and clone
  lineage (parent path, shared SSTs, detached or not).
- **Garbage** — GC health: live / pinned / reclaimable breakdown, which
  checkpoints keep how much storage alive, and recent GC sweeps.

## Running

Everything is one binary. The object store is configured through environment
variables (or an `--env-file`), exactly like `slatedb-cli`:

```sh
# UI + API together (the default; `serve` may be omitted)
CLOUD_PROVIDER=local LOCAL_PATH=/path/to/store \
  slatedb-dashboard --path my-db

# S3
CLOUD_PROVIDER=aws AWS_BUCKET=my-bucket ... \
  slatedb-dashboard serve --path my-db --listen 0.0.0.0:8333

# REST API only (no UI). CORS defaults to '*' in this mode so a ui-only
# instance can call it from the browser; restrict with --cors-allow-origin.
slatedb-dashboard serve --api-only --path my-db

# UI only: serves just the SPA, with the API base baked into index.html —
# the browser calls that API directly. No object-store config needed here.
slatedb-dashboard serve --ui-only --api-url http://api-host:8333
```

Serve flags: `--path` (DB root within the store; required unless
`--ui-only`), `--listen` (default `127.0.0.1:8333`), `--env-file`,
`--cache-ttl-secs` (default 5 — object-store reads of mutable state are
cached and shared across viewers, so polling cost stays bounded),
`--api-only` / `--ui-only --api-url URL`, `--cors-allow-origin` (repeatable).

## Demo

```sh
# Seed ./demo-data with a local DB if it doesn't exist yet, then simulate
# live traffic against it until Ctrl-C (this is the only mode that writes;
# the dashboard itself never does): puts/deletes at a slowly swinging rate,
# embedded compactor and GC enabled, short-lived checkpoints every couple
# of minutes.
cargo run -- traffic                         # --rate, --checkpoint-secs

# Then watch it:
CLOUD_PROVIDER=local LOCAL_PATH=$(pwd)/demo-data cargo run -- --path demo-db
```

Note: `LOCAL_PATH` must be absolute — the object store canonicalizes it.

## Development

```sh
npm run dev --prefix web    # Vite dev server on :5173, proxies /api to :8333
cargo test                  # unit tests
./scripts/smoke.sh          # curl every endpoint against a running server

# Release binary with the frontend embedded (single-file deploy):
npm run build --prefix web && cargo build --release
```
