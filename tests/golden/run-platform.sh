#!/bin/bash
# Platform-specific golden test runner
# Usage: ./run-platform.sh --platform PLATFORM [--model MODEL] [--base-url URL] [--api-key KEY] [--parallel N]
#
# Platforms: python-webapp, react-webapp, go-webapp, python-desktop, react-mobile, all
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

MODEL="vllm/qwen3.5-9b"
BASE_URL="http://192.168.3.60:4000"
API_KEY=""
PLATFORM=""
PARALLEL=1
DELAY=2
TIMEOUT=120

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2;;
    --base-url) BASE_URL="$2"; shift 2;;
    --api-key) API_KEY="$2"; shift 2;;
    --platform) PLATFORM="$2"; shift 2;;
    --parallel) PARALLEL="$2"; shift 2;;
    --delay) DELAY="$2"; shift 2;;
    --timeout) TIMEOUT="$2"; shift 2;;
    *) echo "Unknown: $1"; exit 1;;
  esac
done

[ -z "$API_KEY" ] && { echo "Error: --api-key required"; exit 1; }
[ -z "$PLATFORM" ] && { echo "Error: --platform required (python-webapp|react-webapp|go-webapp|python-desktop|react-mobile|all)"; exit 1; }

SCENARIOS="$SCRIPT_DIR/scenarios/platform-scenarios.json"
MUSL_BIN="$SCRIPT_DIR/../../mc/target/x86_64-unknown-linux-musl/release/magic-code"
TIMESTAMP=$(date -u +%Y%m%d-%H%M%S)
SAFE_MODEL=$(echo "$MODEL" | tr '/:' '-')
DOCKER_IMAGE="redis:alpine"

[ ! -f "$MUSL_BIN" ] && { echo "Error: musl binary not found"; exit 1; }

MC_CMD="timeout $TIMEOUT magic-code --base-url $BASE_URL --api-key $API_KEY --model $MODEL --yes --json"

run_one() {
  local platform="$1" id="$2" prompt="$3" setup_cmd="${4:-}"
  local fixture="$SCRIPT_DIR/fixtures/${platform}/setup.sh"
  local ws="/tmp/mc-golden-${id}"
  local ctr="mc-golden-${id}"

  bash "$fixture" "$ws" >/dev/null 2>&1
  [ -n "$setup_cmd" ] && (cd "$ws" && eval "$setup_cmd") 2>/dev/null

  sudo docker rm -f "$ctr" >/dev/null 2>&1 || true
  sudo docker run -d --name "$ctr" \
    -v "$MUSL_BIN:/usr/local/bin/magic-code:ro" \
    -v "$ws:/workspace" -w /workspace \
    --network host --entrypoint sleep "$DOCKER_IMAGE" 600 >/dev/null 2>&1
  sleep 1

  local t0=$(date +%s)
  local raw
  raw=$(sudo docker exec "$ctr" sh -c "$MC_CMD \"$prompt\"" 2>&1) || true
  local dur=$(( $(date +%s) - t0 ))

  local tools=$(echo "$raw" | grep '"type":"tool_call"' | sed 's/.*"name":"\([^"]*\)".*/\1/' | tr '\n' ',' | sed 's/,$//')
  local tin=$(echo "$raw" | grep -o '"input_tokens": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local tout=$(echo "$raw" | grep -o '"output_tokens": [0-9]*' | tail -1 | grep -o '[0-9]*')

  python3 -c "
import json,sys
r={'id':sys.argv[1],'platform':sys.argv[2],'model':sys.argv[3],'prompt':sys.argv[4],
   'tools':sys.argv[5].split(',') if sys.argv[5] else [],
   'input_tokens':int(sys.argv[6] or 0),'output_tokens':int(sys.argv[7] or 0),
   'duration_sec':int(sys.argv[8]),'has_output':int(sys.argv[7] or 0)>0,'timestamp':sys.argv[9]}
print(json.dumps(r))
" "$id" "$platform" "$MODEL" "$prompt" "$tools" "${tin:-0}" "${tout:-0}" "$dur" "$TIMESTAMP" \
    >> "$RESULT_FILE"

  local st="✅"; [ -z "$tools" ] && [ "${tout:-0}" = "0" ] && st="❌"
  printf "%s %-6s %-18s %4ss %6sin %4sout  %s\n" "$st" "$id" "$platform" "$dur" "${tin:-0}" "${tout:-0}" "${tools:-(none)}"

  sudo docker rm -f "$ctr" >/dev/null 2>&1 || true
  rm -rf "$ws"
  sleep "$DELAY"
}
export -f run_one

# Get platforms to run
if [ "$PLATFORM" = "all" ]; then
  PLATFORMS=$(python3 -c "import json; d=json.load(open('$SCENARIOS')); print(' '.join(d['platforms'].keys()))")
else
  PLATFORMS="$PLATFORM"
fi

for plat in $PLATFORMS; do
  RESULT_FILE="$SCRIPT_DIR/results/${SAFE_MODEL}-${plat}-${TIMESTAMP}.jsonl"
  echo "============================================"
  echo " Platform: $plat | Model: $MODEL | Parallel: $PARALLEL"
  echo " Output: $RESULT_FILE"
  echo "============================================"

  SCENARIO_LIST=$(python3 -c "
import json
with open('$SCENARIOS') as f:
    data = json.load(f)
p = data['platforms']['$plat']
for s in p['scenarios']:
    setup = s.get('setup', '')
    prompt = s['prompt'].replace('\"', '\\\\\"')
    print(f\"$plat|{s['id']}|{prompt}|{setup}\")
")

  if [ "$PARALLEL" -le 1 ]; then
    echo "$SCENARIO_LIST" | while IFS='|' read -r plat id prompt setup; do
      run_one "$plat" "$id" "$prompt" "$setup"
    done
  else
    RUNNING=0
    echo "$SCENARIO_LIST" | while IFS='|' read -r plat id prompt setup; do
      run_one "$plat" "$id" "$prompt" "$setup" &
      RUNNING=$((RUNNING + 1))
      if [ "$RUNNING" -ge "$PARALLEL" ]; then
        wait -n 2>/dev/null || true
        RUNNING=$((RUNNING - 1))
      fi
    done
    wait
  fi

  echo ""
  python3 -c "
import json
results=[json.loads(l) for l in open('$RESULT_FILE')]
ok=sum(1 for r in results if r['has_output'])
print(f'  {len(results)} scenarios | {ok} pass ({ok*100//max(len(results),1)}%)')
"
  echo ""
done
