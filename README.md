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

Everything is one binary, and DBs are **auto-discovered**: the dashboard
walks the configured object store(s) and detects a SlateDB wherever a
prefix has a `manifest/` directory with manifest files in it. The fleet
page lists every discovered DB; each DB gets its own URLs under `/db/{id}`.

```sh
# Single store from ambient env vars (exactly like slatedb-cli); scans the
# whole store by default, or scoped prefixes via --root (repeatable).
CLOUD_PROVIDER=local LOCAL_PATH=/path/to/store slatedb-dashboard
CLOUD_PROVIDER=aws AWS_BUCKET=my-bucket ... slatedb-dashboard serve --root dbs/

# Multiple stores via a self-contained TOML config:
slatedb-dashboard serve --config stores.toml

# REST API only (no UI). CORS defaults to '*' in this mode so a ui-only
# instance can call it from the browser; restrict with --cors-allow-origin.
slatedb-dashboard serve --api-only

# UI only: serves just the SPA, with the API base baked into index.html —
# the browser calls that API directly. No object-store config needed here.
slatedb-dashboard serve --ui-only --api-url http://api-host:8333
```

`stores.toml` carries each store's provider settings inline, keyed by the
documented env-var names lowercased. Values may reference ambient env vars
with `${VAR}` — that's how multiple stores of the same provider use
different credentials without putting secrets in the file (unset keys also
fall through to the ambient env):

```toml
[[stores]]
name = "local"
provider = "local"            # local | memory | aws | azure
local_path = "/data/store"
roots = [""]                  # prefixes to scan (default: the store root)

[[stores]]
name = "prod"
provider = "aws"
aws_bucket = "prod-bucket"
aws_region = "us-east-1"
aws_access_key_id = "${PROD_AWS_KEY_ID}"
aws_secret_access_key = "${PROD_AWS_SECRET}"
roots = ["dbs/"]
```

Serve flags: `--config FILE` or `--root PREFIX` (repeatable), `--listen`
(default `127.0.0.1:8333`), `--cache-ttl-secs` (default 5 — object-store
reads of mutable state are cached and shared across viewers, so polling
cost stays bounded), `--scan-depth` (default 4) / `--scan-ttl-secs`
(default 60) for discovery, `--api-only` / `--ui-only --api-url URL`,
`--cors-allow-origin` (repeatable).

## Demo

```sh
# Seed three demo DBs into ./demo-data if missing, then simulate live
# traffic against all of them concurrently until Ctrl-C (this is the only
# mode that writes; the dashboard itself never does). Each DB runs at a
# different rate and phase so the fleet looks heterogeneous.
cargo run -- traffic                         # --dbs, --rate, --checkpoint-secs
# add --clean to delete ./demo-data first and start from scratch

# Then watch them (the fleet page lists all three):
CLOUD_PROVIDER=local LOCAL_PATH=$(pwd)/demo-data cargo run
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
