#!/usr/bin/env bash
# Smoke-tests every API endpoint against a running dashboard, exercising
# the first discovered DB.
set -euo pipefail

HOST="${1:-127.0.0.1:8333}"
BASE="http://$HOST/api"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

get() {
  curl -fsS "$BASE$1" || fail "GET $1"
}

echo "smoke-testing $BASE ..."

get /health | jq -e '.status == "ok" and .store_count >= 1' > /dev/null || fail "health status"
get /dbs | jq -e '(.dbs | length) >= 1 and has("scanned_at")' > /dev/null || fail "dbs listing"

# All per-DB endpoints live under /dbs/{id}; use the first discovered DB.
ID=$(get /dbs | jq -r '.dbs[0].id')
ENC=$(jq -rn --arg id "$ID" '$id | @uri')
db() {
  curl -fsS "$BASE/dbs/$ENC$1" || fail "GET /dbs/$ID$1"
}
echo "  using db '$ID'"

db /overview | jq -e '.manifest_id >= 1 and .sst_count >= 0 and (.warnings | type == "array")' > /dev/null || fail "overview invariants"
db /lsm | jq -e '.tree | has("l0") and has("runs")' > /dev/null || fail "lsm shape"
db /lsm/summary | jq -e '(.levels | type == "array") and (.levels[0].coverage | length) == .buckets' > /dev/null || fail "lsm summary shape"
db /wal | jq -e 'has("next_wal_sst_id") and (.entries | type == "array")' > /dev/null || fail "wal shape"

LATEST=$(db /overview | jq .manifest_id)
db /manifests/ids | jq -e 'length >= 1' > /dev/null || fail "manifest ids"
db "/lsm?manifest_id=$LATEST" | jq -e ".manifest_id == $LATEST" > /dev/null || fail "lsm time travel"
db "/lsm/summary?manifest_id=$LATEST" | jq -e ".manifest_id == $LATEST" > /dev/null || fail "lsm summary time travel"
db "/manifests?limit=10" | jq -e 'length >= 1' > /dev/null || fail "manifests list"
db "/manifests/$LATEST" | jq -e ".id == $LATEST" > /dev/null || fail "manifest by id"
db /manifests/latest | jq -e ".id == $LATEST" > /dev/null || fail "manifest latest"

OLDEST=$(db "/manifests?limit=500" | jq '.[-1].id')
if [ "$OLDEST" != "$LATEST" ]; then
  db "/manifests/diff?a=$OLDEST&b=$LATEST" | jq -e ".a == $OLDEST and .b == $LATEST" > /dev/null || fail "manifest diff"
fi

# Drill into the first compacted SST we can find (L0 or any sorted run,
# root tree or segments).
ULID=$(db /lsm | jq -r '[.tree.l0[].sst_id, .tree.runs[].ssts[].sst_id, (.segments[]?.tree | .l0[].sst_id, .runs[].ssts[].sst_id)] | map(select(.kind == "compacted")) | .[0].ulid // empty')
if [ -n "$ULID" ]; then
  db "/ssts/$ULID" | jq -e '.size_bytes > 0 and (.index.total_blocks >= 1)' > /dev/null || fail "sst detail"
else
  echo "  (no compacted SSTs to drill into)"
fi

db "/activity?limit=5" | jq -e 'type == "array"' > /dev/null || fail "activity"
db /compactor/state | jq -e 'has("manifest_id")' > /dev/null || fail "compactor state"
db /compactions | jq -e 'type == "array"' > /dev/null || fail "compactions list"
db /checkpoints | jq -e 'type == "array"' > /dev/null || fail "checkpoints"
db /clones | jq -e 'type == "array"' > /dev/null || fail "clones"
db /garbage | jq -e '.stored_bytes >= 0 and (.compacted.stored_count >= 0) and (.stored_bytes == .live_bytes + .pinned_bytes + .reclaimable_bytes)' > /dev/null || fail "garbage invariants"

# Search round-trip: any compacted SST should find its object + references.
SULID=$(db /lsm | jq -r '[.tree.l0[].sst_id, .tree.runs[].ssts[].sst_id, (.segments[]?.tree | .l0[].sst_id, .runs[].ssts[].sst_id)] | map(select(.kind == "compacted")) | .[0].ulid // empty')
if [ -n "$SULID" ]; then
  db "/search?q=$SULID" | jq -e '.sst_object != null and (.manifests | length) >= 1' > /dev/null || fail "search"
fi

# Unknown DB ids must 404.
STATUS=$(curl -sS -o /dev/null -w '%{http_code}' "$BASE/dbs/nope%3Amissing/overview")
[ "$STATUS" = "404" ] || fail "unknown db 404 (got $STATUS)"

# /metrics is root-level (Prometheus convention), not under /api.
curl -fsS "http://$HOST/metrics" | grep -q '^slatedb_up' || fail "metrics"

# OpenAPI document and reference UI.
curl -fsS "$BASE/openapi.json" | jq -e '.openapi and (.paths | length) >= 20' > /dev/null || fail "openapi.json"
curl -fsS "$BASE/docs" | grep -q "api-reference" || fail "docs page"

echo "all endpoints OK"
