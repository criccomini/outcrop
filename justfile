# SlateDB dashboard development tasks

# Address the dev server / smoke tests target
listen := "127.0.0.1:8333"

default:
    @just --list

# Seed a local demo DB into ./demo-data
seed *ARGS:
    cargo run --bin seed -- {{ARGS}}

# Run the dashboard against the demo DB
run *ARGS:
    CLOUD_PROVIDER=local LOCAL_PATH=./demo-data cargo run -- --path demo-db --listen {{listen}} {{ARGS}}

# Seed then run
demo: seed run

# Frontend dev server (proxies /api to the Rust backend)
dev-web:
    npm run dev --prefix web

# Build the frontend bundle into web/dist
build-web:
    npm run build --prefix web

# Full release build (frontend embedded in the binary)
build: build-web
    cargo build --release

# Unit tests
test:
    cargo test

# Curl every API endpoint against a running server and check invariants
smoke:
    ./scripts/smoke.sh {{listen}}
