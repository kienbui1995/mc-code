#!/bin/bash
# Golden test runner for mc-code
# Usage: ./run.sh [--model MODEL] [--base-url URL] [--api-key KEY] [--category CAT] [--docker-image IMG]
#
# Runs all scenarios from scenarios.json, captures structured results.
# Each scenario gets a fresh project state (fixture reset).
#
# Output: results/<model>-<timestamp>.jsonl

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Defaults
MODEL="vllm/qwen3.5-9b"
BASE_URL="http://192.168.3.60:4000"
API_KEY=""
CATEGORY=""
DOCKER_IMAGE="redis:alpine"
CONTAINER="mc-golden-test"
DELAY=3
TIMEOUT=120

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2;;
    --base-url) BASE_URL="$2"; shift 2;;
    --api-key) API_KEY="$2"; shift 2;;
    --category) CATEGORY="$2"; shift 2;;
    --docker-image) DOCKER_IMAGE="$2"; shift 2;;
    --delay) DELAY="$2"; shift 2;;
    --timeout) TIMEOUT="$2"; shift 2;;
    *) echo "Unknown: $1"; exit 1;;
  esac
done

if [ -z "$API_KEY" ]; then
  echo "Error: --api-key required"
  exit 1
fi

# Paths
FIXTURE="$SCRIPT_DIR/fixtures/setup.sh"
SCENARIOS="$SCRIPT_DIR/scenarios/scenarios.json"
WORKSPACE="/tmp/mc-golden-workspace"
MUSL_BIN="$SCRIPT_DIR/../../mc/target/x86_64-unknown-linux-musl/release/magic-code"

TIMESTAMP=$(date -u +%Y%m%d-%H%M%S)
SAFE_MODEL=$(echo "$MODEL" | tr '/:' '-')
RESULT_FILE="$SCRIPT_DIR/results/${SAFE_MODEL}-${TIMESTAMP}.jsonl"
mkdir -p "$SCRIPT_DIR/results"

echo "============================================"
echo " mc-code Golden Test Runner"
echo " Model:  $MODEL"
echo " URL:    $BASE_URL"
echo " Output: $RESULT_FILE"
echo "============================================"

# Check binary
if [ ! -f "$MUSL_BIN" ]; then
  echo "Error: musl binary not found at $MUSL_BIN"
  echo "Run: cd mc && cargo build --release --target x86_64-unknown-linux-musl"
  exit 1
fi

MC_CMD="timeout $TIMEOUT magic-code --base-url $BASE_URL --api-key $API_KEY --model $MODEL --yes --json"

run_scenario() {
  local id="$1"
  local category="$2"
  local prompt="$3"
  local extra_flags="${4:-}"
  local setup_cmd="${5:-}"

  # Reset fixture
  bash "$FIXTURE" "$WORKSPACE" >/dev/null 2>&1

  # Apply scenario-specific setup
  if [ -n "$setup_cmd" ]; then
    (cd "$WORKSPACE" && eval "$setup_cmd") 2>/dev/null
  fi

  # Recreate container
  sudo docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  sudo docker run -d --name "$CONTAINER" \
    -v "$MUSL_BIN:/usr/local/bin/magic-code:ro" \
    -v "$WORKSPACE:/workspace" \
    -w /workspace \
    --network host \
    --entrypoint sleep \
    "$DOCKER_IMAGE" 3600 >/dev/null 2>&1

  sleep 1

  # Run mc-code
  local start_time=$(date +%s)
  local raw_output
  raw_output=$(sudo docker exec "$CONTAINER" sh -c \
    "$MC_CMD $extra_flags \"$prompt\"" 2>&1) || true
  local end_time=$(date +%s)
  local duration=$((end_time - start_time))

  # Parse results
  local tools=$(echo "$raw_output" | grep '"type":"tool_call"' | \
    sed 's/.*"name":"\([^"]*\)".*/\1/' | tr '\n' ',' | sed 's/,$//')
  local input_tokens=$(echo "$raw_output" | grep -o '"input_tokens": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local output_tokens=$(echo "$raw_output" | grep -o '"output_tokens": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local iterations=$(echo "$raw_output" | grep -o '"iterations": [0-9]*' | tail -1 | grep -o '[0-9]*')
  local text=$(echo "$raw_output" | grep -o '"text": ".*"' | tail -1 | head -c 500)

  # Capture file state after run
  local files_changed=$(sudo docker exec "$CONTAINER" \
    find /workspace/src /workspace/tests -name "*.rs" -newer /workspace/Cargo.toml 2>/dev/null | \
    tr '\n' ',' | sed 's/,$//')

  # Write result
  python3 -c "
import json, sys
r = {
    'id': '$id',
    'category': '$category',
    'model': '$MODEL',
    'prompt': '''$prompt''',
    'tools': '${tools}'.split(',') if '${tools}' else [],
    'input_tokens': int('${input_tokens:-0}' or 0),
    'output_tokens': int('${output_tokens:-0}' or 0),
    'iterations': int('${iterations:-0}' or 0),
    'duration_sec': $duration,
    'files_changed': '${files_changed}'.split(',') if '${files_changed}' else [],
    'has_output': int('${output_tokens:-0}' or 0) > 0,
    'timestamp': '$TIMESTAMP'
}
print(json.dumps(r))
" >> "$RESULT_FILE"

  # Print summary
  local status="✅"
  [ -z "$tools" ] && [ "${output_tokens:-0}" = "0" ] && status="❌"
  printf "%s %-6s %-18s %5ss %6sin %4sout  %s\n" \
    "$status" "$id" "$category" "$duration" "${input_tokens:-0}" "${output_tokens:-0}" "${tools:-(none)}"

  sleep "$DELAY"
}

# Parse and run scenarios
echo ""
python3 -c "
import json, sys

with open('$SCENARIOS') as f:
    data = json.load(f)

for cat_name, cat in data['categories'].items():
    if '$CATEGORY' and cat_name != '$CATEGORY':
        continue
    for s in cat['scenarios']:
        extra = s.get('extra_flags', '')
        setup = s.get('setup', '')
        print(f\"{s['id']}|{cat_name}|{s['prompt']}|{extra}|{setup}\")
" | while IFS='|' read -r id category prompt extra setup; do
  run_scenario "$id" "$category" "$prompt" "$extra" "$setup"
done

# Cleanup
sudo docker rm -f "$CONTAINER" >/dev/null 2>&1 || true

echo ""
echo "============================================"
echo " Results: $RESULT_FILE"
echo " Total: $(wc -l < "$RESULT_FILE") scenarios"
echo "============================================"

# Summary
python3 -c "
import json
results = [json.loads(l) for l in open('$RESULT_FILE')]
total = len(results)
has_tools = sum(1 for r in results if r['tools'] and r['tools'] != [''])
has_output = sum(1 for r in results if r['has_output'])
total_tokens = sum(r['input_tokens'] for r in results)
total_time = sum(r['duration_sec'] for r in results)
print(f'  With tools: {has_tools}/{total}')
print(f'  With output: {has_output}/{total}')
print(f'  Total tokens: {total_tokens:,}')
print(f'  Total time: {total_time}s')
print(f'  Avg time/scenario: {total_time/max(total,1):.1f}s')
"
