# slatedb-dashboard

A **read-only** web dashboard for inspecting a [SlateDB](https://slatedb.io)
database directly from object storage.

SlateDB keeps all of its state — manifests, WAL SSTs, L0 SSTs, sorted runs,
checkpoints — in the object store, so the dashboard needs no cooperation from
the running writer. It performs **zero writes**: only manifest reads, SST
metadata reads, and object listings.

## Features

- **Overview** — health summary: sizes, SST counts, manifest freshness, WAL
  window, epochs.
- **LSM Tree** — visual tree: levels by size and key-range coverage
  (overlap reads as read amplification), with per-SST drill-down into block
  index, bloom filter/stats sizes, and content stats.
- **Manifests** — full history, structured view of any version, and a
  semantic diff between any two versions ("3 L0 SSTs compacted into SR 7").
- **Compactions** — current compactor state plus history of `.compactions`
  versions.
- **Checkpoints** — checkpoint table with expiry countdowns, and clone
  lineage (parent path, shared SSTs, detached or not).

## Running

The object store is configured through environment variables (or an
`--env-file`), exactly like `slatedb-cli`:

```sh
# Local filesystem
CLOUD_PROVIDER=local LOCAL_PATH=/path/to/store \
  slatedb-dashboard --path my-db

# S3
CLOUD_PROVIDER=aws AWS_BUCKET=my-bucket ... \
  slatedb-dashboard --path my-db --listen 0.0.0.0:8333
```

Flags: `--path` (DB root within the store, required), `--listen`
(default `127.0.0.1:8333`), `--env-file`, `--cache-ttl-secs` (default 5 —
object-store reads of mutable state are cached and shared across viewers, so
polling cost stays bounded).

## Demo

```sh
# Seed ./demo-data with a local DB (this is the only tool here that writes;
# the dashboard itself never does), then serve it:
cargo run --bin seed
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
