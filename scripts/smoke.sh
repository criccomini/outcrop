#!/usr/bin/env bash
# Smoke-tests every API endpoint against a running dashboard.
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

get /health | jq -e '.status == "ok"' > /dev/null || fail "health status"
get /overview | jq -e '.manifest_id >= 1 and .sst_count >= 0 and (.warnings | type == "array")' > /dev/null || fail "overview invariants"
get /lsm | jq -e '.tree | has("l0") and has("runs")' > /dev/null || fail "lsm shape"
get /wal | jq -e 'has("next_wal_sst_id") and (.entries | type == "array")' > /dev/null || fail "wal shape"

LATEST=$(get /overview | jq .manifest_id)
get /manifests/ids | jq -e 'length >= 1' > /dev/null || fail "manifest ids"
get "/lsm?manifest_id=$LATEST" | jq -e ".manifest_id == $LATEST" > /dev/null || fail "lsm time travel"
get "/manifests?limit=10" | jq -e 'length >= 1' > /dev/null || fail "manifests list"
get "/manifests/$LATEST" | jq -e ".id == $LATEST" > /dev/null || fail "manifest by id"
get /manifests/latest | jq -e ".id == $LATEST" > /dev/null || fail "manifest latest"

OLDEST=$(get "/manifests?limit=500" | jq '.[-1].id')
if [ "$OLDEST" != "$LATEST" ]; then
  get "/manifests/diff?a=$OLDEST&b=$LATEST" | jq -e ".a == $OLDEST and .b == $LATEST" > /dev/null || fail "manifest diff"
fi

# Drill into the first compacted SST we can find (L0 or any sorted run).
ULID=$(get /lsm | jq -r '[.tree.l0[].sst_id, .tree.runs[].ssts[].sst_id] | map(select(.kind == "compacted")) | .[0].ulid // empty')
if [ -n "$ULID" ]; then
  get "/ssts/$ULID" | jq -e '.size_bytes > 0 and (.index.total_blocks >= 1)' > /dev/null || fail "sst detail"
else
  echo "  (no compacted SSTs to drill into)"
fi

get "/activity?limit=5" | jq -e 'type == "array"' > /dev/null || fail "activity"
get /compactor/state | jq -e 'has("manifest_id")' > /dev/null || fail "compactor state"
get /compactions | jq -e 'type == "array"' > /dev/null || fail "compactions list"
get /checkpoints | jq -e 'type == "array"' > /dev/null || fail "checkpoints"
get /clones | jq -e 'type == "array"' > /dev/null || fail "clones"
get /garbage | jq -e '.stored_bytes >= 0 and (.compacted.stored_count >= 0) and (.stored_bytes == .live_bytes + .pinned_bytes + .reclaimable_bytes)' > /dev/null || fail "garbage invariants"

# /metrics is root-level (Prometheus convention), not under /api.
curl -fsS "http://$HOST/metrics" | grep -q '^slatedb_up' || fail "metrics"

echo "all endpoints OK"
