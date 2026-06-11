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

- **Works against the bucket, not the database** — reads manifests, SST
  metadata and object listings straight from object storage. Zero writes,
  no agent, no cooperation needed from the running writer.
- **Fleet auto-discovery** — point it at one or more stores (env vars or a
  TOML config) and every SlateDB under them shows up; no per-DB setup.
- **Understands SlateDB, not just files** — semantic manifest diffs
  ("3 L0 SSTs compacted into SR 7"), an activity feed narrating flushes,
  compactions and GC sweeps, garbage classified live / pinned / reclaimable
  by the GC's own rules, and health alerts for L0 backlog, WAL growth,
  stale manifests and stuck GC.
- **Visualizes the LSM** — levels by size and key-range coverage (overlap
  reads as read amplification), per-SST drill-down to block index, bloom
  filter and content stats, segment tabs, and a scrubber that rewinds the
  tree through manifest history — plus pages for WAL, manifests,
  compactions, checkpoints, and cross-resource search.
- **REST API, OpenAPI included** — everything the UI shows is a JSON API,
  documented by an OpenAPI 3.1 spec with an interactive reference at
  `/api/docs`; generate typed clients with any OpenAPI generator.
- **Prometheus metrics** — per-DB gauges at `/metrics`, ready to scrape.
- **Built to be polled** — object-store reads are cached and shared across
  viewers, list endpoints are capped, and huge trees ship as bounded
  summaries, so a wall of open dashboards won't run up your S3 bill.
- **One binary** — the SPA is embedded; or split it into `--api-only` and
  `--ui-only` deployments. Includes a demo traffic generator that can seed
  multi-GiB fleets to explore against.

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
