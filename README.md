# Outcrop

An outcrop is where rock strata surface — visible, layer by layer, without
digging. **Outcrop** is a **read-only** web dashboard that does the same
for [SlateDB](https://slatedb.io): it inspects databases directly from
object storage, layer by layer.

SlateDB keeps all of its state — manifests, WAL SSTs, L0 SSTs, sorted runs,
checkpoints — in the object store, so the dashboard needs no cooperation from
the running writer. It performs **zero writes**: only manifest reads, SST
metadata reads, and object listings.

Built against SlateDB **0.13.x**; older or newer manifest formats may not
decode.

![Overview page: sizes, WAL window, epochs, checkpoints, and the storage & garbage breakdown](docs/overview.png)

![LSM Tree page: levels by size and key-range coverage for a 75 GiB database](docs/lsm.png)

## Features

- **Overview** — health summary: sizes, SST counts, manifest freshness, WAL
  window, epochs, plus a storage & garbage panel (space amplification, bytes
  pinned by checkpoints, and what the GC would reclaim).
- **Alerts** — health warnings (L0 backlog, WAL window growth, stale
  manifests, expired checkpoints the GC never swept) in one place, with a
  count badge in the nav.
- **Activity** — a unified feed, newest first: manifest transitions (runs of
  flushes coalesced, each linking to its diff), the compactor's job log —
  which reaches further back than transitions, since the GC prunes old
  manifests within minutes — and GC deletions observed by the server,
  grouped per sweep and expandable to the individual objects.
- **LSM Tree** — the tree two ways: levels by size, and key-range coverage
  where vertical overlap reads as read amplification. Levels too large to
  draw per-SST render as coverage histograms instead, so the page stays fast
  on huge DBs — clicking a histogram bucket lists the SSTs covering that key
  range. Every SST clicks through to block index, bloom filter and content
  stats. Segmented DBs (RFC-0024) get one tab per segment, and a manifest
  scrubber rewinds the whole view to any retained version.
- **WAL** — the write-ahead log listing with the replay window highlighted.
- **Manifests** — full history, structured view of any version, and a
  semantic diff between any two versions ("3 L0 SSTs compacted into SR 7").
- **Compactions** — what's compacting right now, plus the history of
  `.compactions` versions and per-job drill-down.
- **Checkpoints** — checkpoint table with expiry countdowns, and clone
  lineage (parent path, shared SSTs, detached or not).
- **Garbage** — GC health: live / pinned / reclaimable breakdown, which
  checkpoints keep how much storage alive, and recent GC sweeps.
- **Search** — one box that finds manifests, checkpoints, SSTs and
  compactions by id, ULID, UUID or key.

## Running

Everything is one binary, and DBs are **auto-discovered**: the dashboard
walks the configured object store(s) and detects a SlateDB wherever a
prefix has a `manifest/` directory with manifest files in it. The fleet
page lists every discovered DB; each DB gets its own URLs under `/db/{id}`.

```sh
# Single store from ambient env vars (exactly like slatedb-cli); scans the
# whole store by default, or scoped prefixes via --root (repeatable).
CLOUD_PROVIDER=local LOCAL_PATH=/path/to/store outcrop
CLOUD_PROVIDER=aws AWS_BUCKET=my-bucket ... outcrop serve --root dbs/

# Multiple stores via a self-contained TOML config:
outcrop serve --config stores.toml

# REST API only (no UI). CORS defaults to '*' in this mode so a ui-only
# instance can call it from the browser; restrict with --cors-allow-origin.
outcrop serve --api-only

# UI only: serves just the SPA, with the API base baked into index.html —
# the browser calls that API directly. No object-store config needed here.
outcrop serve --ui-only --api-url http://api-host:8333
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

The dashboard has **no authentication** and, while read-only, exposes DB
metadata (key ranges, checkpoint names, store paths). It binds to
localhost by default; to share it, put it behind a reverse proxy that
handles auth, or at least bind only to a trusted network.

## REST API

Everything the UI shows comes from a JSON API you can use directly:

- `GET /api/dbs` — discovered databases; per-DB resources live under
  `/api/dbs/{db}/…` where `{db}` is the id `{store}:{path}` as a single
  path segment (percent-encode any slashes in the path).
- `GET /api/openapi.json` — OpenAPI 3.1 document covering every endpoint
  and schema. Generate typed clients with any OpenAPI generator, e.g.
  `npx openapi-typescript http://127.0.0.1:8333/api/openapi.json`.
- `GET /api/docs` — interactive API reference rendering that spec (the
  viewer script loads from a CDN; the spec itself is self-contained).
- `GET /metrics` — Prometheus exposition for every discovered DB (sizes,
  SST counts, WAL window, epochs, manifest freshness), root-level by
  convention.

Errors are JSON `{"error": "..."}` with conventional status codes. List
endpoints cap their `limit` parameters server-side because every item can
cost an object-store request.

## Demo

```sh
# Seed three demo DBs into ./demo-data if missing, then simulate live
# traffic against all of them concurrently until Ctrl-C (this is the only
# mode that writes; the dashboard itself never does). Each DB runs at a
# different rate and phase, and randomly (but stably, by name) decides
# whether it's segmented (RFC-0024) so the fleet shows both shapes.
cargo run -- traffic              # --dbs, --rate, --checkpoint-secs, --segments
# add --clean to delete ./demo-data first and start from scratch

# Then watch them (the fleet page lists all three):
CLOUD_PROVIDER=local LOCAL_PATH=$(pwd)/demo-data cargo run
```

Note: `LOCAL_PATH` must be absolute — the object store canonicalizes it.

To exercise the dashboard at scale, `--target-size` switches seeding to
bulk mode: unthrottled batched writes with the embedded compactor and GC
running, until the DB holds that much live data. Bulk seeding is
resumable (progress is measured from the store itself) and bounds its
transient disk use — peak ≈ target + `--max-garbage` (default 32GiB):

```sh
# One 50GiB DB with ~1600 32MiB SSTs, then churn it:
cargo run -- traffic --target-size 50GiB
# Knobs: --value-bytes 4KiB..64KiB, --sst-bytes 32MiB (SST count ≈
# target/sst-bytes), --seed-only (exit after seeding), --no-wal (halve
# seed bytes written), --time-warp (expire the compactor's internal
# 15-minute checkpoints in seconds instead of waiting them out, so
# seeding runs at raw write speed; only safe while nothing reads the DB).
```

## Development

```sh
npm run dev --prefix web    # Vite dev server on :5173, proxies /api to :8333
npx tsc --noEmit            # typecheck the frontend (run inside web/)
cargo test                  # backend unit tests
./scripts/smoke.sh          # curl every endpoint against a running server

# Release binary with the frontend embedded (single-file deploy);
# needs Rust (stable) and Node 18+:
npm run build --prefix web && cargo build --release
```

In debug builds the server reads `web/dist` from disk, so after
`npm run build --prefix web` a running debug server picks up frontend
changes without a cargo rebuild; release builds embed the assets.
