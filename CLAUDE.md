# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A **read-only** web dashboard for inspecting a SlateDB database directly from object storage: a Rust (axum) API server plus a React SPA. The dashboard performs zero writes — only manifest reads, SST metadata reads, and object listings. The only thing in this repo that writes is `src/bin/seed.rs`, which generates the local demo database.

## Workflow

Always commit after making progress (e.g. after each fix or coherent unit of work), using conventional commit syntax (`fix:`, `feat:`, `chore:`, `docs:`, with an optional scope like `fix(web):`).

## Commands

```sh
# Seed ./demo-data (once; pass --force to reseed), then run the server
cargo run --bin seed
CLOUD_PROVIDER=local LOCAL_PATH=$(pwd)/demo-data cargo run -- --path demo-db
# LOCAL_PATH must be absolute. Serves UI + API on 127.0.0.1:8333.

cargo test                          # backend unit tests
cargo test diff::                   # single module (also: convert::, cache::)
./scripts/smoke.sh                  # curl every endpoint against a running server

npm run dev --prefix web            # Vite dev server on :5173, proxies /api to :8333
npm run build --prefix web          # tsc -b && vite build → web/dist
npx tsc --noEmit                    # typecheck only (run inside web/)

# Single-binary release (frontend embedded via rust-embed)
npm run build --prefix web && cargo build --release
```

In **debug** builds rust-embed serves `web/dist` from disk at runtime, so after `npm run build --prefix web` a running debug server picks up frontend changes without a cargo rebuild. Release builds embed the assets at compile time.

Object store configuration comes from environment variables exactly like `slatedb-cli` (`CLOUD_PROVIDER=local|memory|aws|azure|opendal` plus provider-specific vars), or `--env-file`.

## Architecture

Request flow: `src/routes/*` (one file per page/resource, wired in `routes/mod.rs`) → `AppState` (`src/state.rs`) → slatedb `Admin`/`SstReader` or raw object-store listings → DTOs.

- **Caching (`src/state.rs`, `src/cache.rs`)** is the core design constraint: every uncached request costs object-store GETs/LISTs, and many viewers may poll. Mutable state (latest manifest, manifest listing, compactor state) sits behind `TtlCache` (default 5s, `--cache-ttl-secs`); its mutex is held across the refresh deliberately, so concurrent callers share one fetch. Objects that are immutable once written (manifest by id, SST detail by ULID) go in `LruMap` and are cached forever.
- **DTO layer (`src/convert.rs` → `src/dto.rs`)**: slatedb domain types are converted to serde DTOs; `web/src/api/types.ts` mirrors `dto.rs` field-for-field and must be kept in sync. Rust `Option` fields use `skip_serializing_if`, so the TS side declares them optional (`?`), not `| null`. Keys are always sent as `KeyDto { hex, utf8? }` — `utf8` only when printable.
- **Diff (`src/diff.rs`)**: pure function diffing two manifest DTOs keyed on stable identifiers (L0 view ULIDs, sorted-run ids, checkpoint UUIDs); unit-tested in-file.
- **Errors (`src/error.rs`)**: `ApiError` → JSON `{"error": ...}` with proper status. Do not put underlying object-store errors in client-facing messages — they embed server-side paths; log at `debug` and return a generic message (see `routes/ssts.rs`).
- **Static serving (`src/assets.rs`)**: unknown non-API paths fall back to `index.html` for client-side routing; `/api/*` paths must 404 as JSON instead.
- **List endpoints** cap their `limit` (manifests 500, compactions 200, both clamped to ≥1) and must keep it enforced for every parameter combination — each manifest summary or compactions version in a range costs one object-store GET.
- **Frontend**: react-query hooks in `web/src/api/client.ts` (live pages poll at `LIVE_REFETCH_MS`), pages in `web/src/pages/`, shared rendering in `web/src/components/` (`QueryGate` wraps loading/error states, `KeyDisplay`/`keyText` render `KeyDto`s).

## slatedb crate gotcha

The pinned `slatedb` crates.io release does not match the same-named git tag (e.g. 0.13.1 ≠ `v0.13.1`). When checking what a slatedb API actually does, read the vendored source under `~/.cargo/registry/src/*/slatedb-*/`, not the GitHub repo. To develop against a local checkout, use the commented `[patch.crates-io]` in `Cargo.toml`.
