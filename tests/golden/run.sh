#!/bin/bash
# Golden test runner for mc-code
# Usage: ./run.sh [--model MODEL] [--base-url URL] [--api-key KEY] [--category CAT] [--parallel N]
#
# Runs all scenarios from scenarios.json, captures structured results.
# Each scenario gets a fresh project state (fixture reset).
# --parallel N runs N scenarios concurrently (default: 1, max recommended: 4)
#
# Output: results/<model>-<timestamp>.jsonl

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

MODEL="vllm/qwen3.5-9b"
BASE_URL="http://192.168.3.60:4000"
API_KEY=""
CATEGORY=""
DOCKER_IMAGE="redis:alpine"
DELAY=2
TIMEOUT=120
PARALLEL=1

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2;;
    --base-url) BASE_URL="$2"; shift 2;;
    --api-key) API_KEY="$2"; shift 2;;
    --category) CATEGORY="$2"; shift 2;;
    --docker-image) DOCKER_IMAGE="$2"; shift 2;;
    --delay) DELAY="$2"; shift 2;;
    --timeout) TIMEOUT="$2"; shift 2;;
    --parallel) PARALLEL="$2"; shift 2;;
    *) echo "Unknown: $1"; exit 1;;
  esac
done

[ -z "$API_KEY" ] && { echo "Error: --api-key required"; exit 1; }

FIXTURE="$SCRIPT_DIR/fixtures/setup.sh"
SCENARIOS="$SCRIPT_DIR/scenarios/scenarios.json"
MUSL_BIN="$SCRIPT_DIR/../../mc/target/x86_64-unknown-linux-musl/release/magic-code"
TIMESTAMP=$(date -u +%Y%m%d-%H%M%S)
SAFE_MODEL=$(echo "$MODEL" | tr '/:' '-')
RESULT_FILE="$SCRIPT_DIR/results/${SAFE_MODEL}-${TIMESTAMP}.jsonl"
RESULT_DIR="$SCRIPT_DIR/results/tmp-${TIMESTAMP}"
mkdir -p "$SCRIPT_DIR/results" "$RESULT_DIR"

[ ! -f "$MUSL_BIN" ] && { echo "Error: musl binary not found. Run: cd mc && cargo build --release --target x86_64-unknown-linux-musl"; exit 1; }

echo "============================================"
echo " mc-code Golden Test Runner"
echo " Model:    $MODEL"
echo " URL:      $BASE_URL"
echo " Parallel: $PARALLEL"
echo " Output:   $RESULT_FILE"
echo "============================================"
echo ""

MC_CMD="timeout $TIMEOUT magic-code --base-url $BASE_URL --api-key $API_KEY --model $MODEL --yes --json"

# Export for subshells
export MODEL BASE_URL API_KEY TIMEOUT DELAY DOCKER_IMAGE MUSL_BIN FIXTURE MC_CMD RESULT_DIR TIMESTAMP

run_one() {
  local id="$1" category="$2" prompt="$3" extra_flags="${4:-}" setup_cmd="${5:-}"
  local ws="/tmp/mc-golden-${id}"
  local ctr="mc-golden-${id}"

  # Setup
  bash "$FIXTURE" "$ws" >/dev/null 2>&1
  [ -n "$setup_cmd" ] && (cd "$ws" && eval "$setup_cmd") 2>/dev/null

  sudo docker rm -f "$ctr" >/dev/null 2>&1 || true
  sudo docker run -d --name "$ctr" \
    -v "$MUSL_BIN:/usr/local/bin/magic-code:ro" \
    -v "$ws:/workspace" -w /workspace \
    --network host --entrypoint sleep \
    "$DOCKER_IMAGE" 600 >/dev/null 2>&1
  sleep 1

  # Run
  local t0=$(date +%s)
  local raw
  raw=$(sudo docker exec "$ctr" sh -c "$MC_CMD $extra_flags \"$prompt\"" 2>&1) || true
  local t1=$(date +%s)
  local dur=$((t1 - t0))

  # Parse
  local tools=$(echo "$raw" | grep '"type":"tool_call"' | sed 's/.*"name":"\([^"]*\)".*/\1/' | tr '\n' ',' | sed 's/,$//')
  local tin=$(echo "$raw" | grep -o '"input_tokens": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local tout=$(echo "$raw" | grep -o '"output_tokens": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local iters=$(echo "$raw" | grep -o '"iterations": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local changed=$(sudo docker exec "$ctr" find /workspace/src /workspace/tests -name "*.rs" -newer /workspace/Cargo.toml 2>/dev/null | tr '\n' ',' | sed 's/,$//')

  # Write result
  python3 -c "
import json,sys
r={'id':sys.argv[1],'category':sys.argv[2],'model':sys.argv[3],'prompt':sys.argv[4],
   'tools':sys.argv[5].split(',') if sys.argv[5] else [],
   'input_tokens':int(sys.argv[6] or 0),'output_tokens':int(sys.argv[7] or 0),
   'iterations':int(sys.argv[8] or 0),'duration_sec':int(sys.argv[9]),
   'files_changed':sys.argv[10].split(',') if sys.argv[10] else [],
   'has_output':int(sys.argv[7] or 0)>0,'timestamp':sys.argv[11]}
print(json.dumps(r))
" "$id" "$category" "$MODEL" "$prompt" "$tools" "${tin:-0}" "${tout:-0}" "${iters:-0}" "$dur" "$changed" "$TIMESTAMP" \
    > "$RESULT_DIR/${id}.jsonl"

  # Print
  local st="✅"; [ -z "$tools" ] && [ "${tout:-0}" = "0" ] && st="❌"
  printf "%s %-6s %-22s %4ss %6sin %4sout  %s\n" "$st" "$id" "$category" "$dur" "${tin:-0}" "${tout:-0}" "${tools:-(none)}"

  # Cleanup
  sudo docker rm -f "$ctr" >/dev/null 2>&1 || true
  rm -rf "$ws"
  sleep "$DELAY"
}
export -f run_one

# Generate scenario list
SCENARIO_LIST=$(python3 -c "
import json
with open('$SCENARIOS') as f:
    data = json.load(f)
for cat_name, cat in data['categories'].items():
    if '$CATEGORY' and cat_name != '$CATEGORY':
        continue
    for s in cat['scenarios']:
        extra = s.get('extra_flags', '')
        setup = s.get('setup', '')
        prompt = s['prompt'].replace('\"', '\\\\\"')
        print(f\"{s['id']}|{cat_name}|{prompt}|{extra}|{setup}\")
")

TOTAL=$(echo "$SCENARIO_LIST" | wc -l)
echo "Running $TOTAL scenarios (parallel=$PARALLEL)..."
echo ""

if [ "$PARALLEL" -le 1 ]; then
  # Sequential
  echo "$SCENARIO_LIST" | while IFS='|' read -r id cat prompt extra setup; do
    run_one "$id" "$cat" "$prompt" "$extra" "$setup"
  done
else
  # Parallel using background jobs
  RUNNING=0
  echo "$SCENARIO_LIST" | while IFS='|' read -r id cat prompt extra setup; do
    run_one "$id" "$cat" "$prompt" "$extra" "$setup" &
    RUNNING=$((RUNNING + 1))
    if [ "$RUNNING" -ge "$PARALLEL" ]; then
      wait -n 2>/dev/null || true
      RUNNING=$((RUNNING - 1))
    fi
  done
  wait
fi

# Merge results
cat "$RESULT_DIR"/*.jsonl 2>/dev/null | sort > "$RESULT_FILE"
rm -rf "$RESULT_DIR"

echo ""
echo "============================================"
echo " Results: $RESULT_FILE"
echo " Total: $(wc -l < "$RESULT_FILE") scenarios"
echo "============================================"

python3 -c "
import json
results = [json.loads(l) for l in open('$RESULT_FILE')]
total = len(results)
ok = sum(1 for r in results if r['has_output'])
tools = sum(1 for r in results if r['tools'] and r['tools'] != [''])
ttok = sum(r['input_tokens'] for r in results)
tsec = sum(r['duration_sec'] for r in results)
print(f'  Pass: {ok}/{total} ({ok*100//max(total,1)}%)')
print(f'  With tools: {tools}/{total}')
print(f'  Total tokens: {ttok:,}')
print(f'  Wall time: {tsec}s')
print(f'  Avg: {tsec/max(total,1):.1f}s/scenario')
"
