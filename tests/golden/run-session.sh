#!/bin/bash
# Multi-turn session test runner
# Usage: ./run-session.sh --model MODEL --base-url URL --api-key KEY [--session NAME] [--timeout N]
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

MODEL="vllm/qwen3.5-9b"
BASE_URL="http://192.168.3.60:4000"
API_KEY=""
SESSION=""
TIMEOUT=600
DELAY=2

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2;;
    --base-url) BASE_URL="$2"; shift 2;;
    --api-key) API_KEY="$2"; shift 2;;
    --session) SESSION="$2"; shift 2;;
    --timeout) TIMEOUT="$2"; shift 2;;
    --delay) DELAY="$2"; shift 2;;
    *) echo "Unknown: $1"; exit 1;;
  esac
done

[ -z "$API_KEY" ] && { echo "Error: --api-key required"; exit 1; }

SCENARIOS="$SCRIPT_DIR/scenarios/session-scenarios.json"
MUSL_BIN="$SCRIPT_DIR/../../mc/target/x86_64-unknown-linux-musl/release/magic-code"
TIMESTAMP=$(date -u +%Y%m%d-%H%M%S)
SAFE_MODEL=$(echo "$MODEL" | tr '/:' '-')
DOCKER_IMAGE="redis:alpine"

[ ! -f "$MUSL_BIN" ] && { echo "Error: musl binary not found"; exit 1; }

# Get sessions to run
SESSIONS=$(python3 -c "
import json
with open('$SCENARIOS') as f:
    data = json.load(f)
for name in data['sessions']:
    if not '$SESSION' or name == '$SESSION':
        print(name)
")

for sess_name in $SESSIONS; do
  RESULT_FILE="$SCRIPT_DIR/results/${SAFE_MODEL}-session-${sess_name}-${TIMESTAMP}.jsonl"

  # Extract session config
  eval "$(python3 -c "
import json
with open('$SCENARIOS') as f:
    s = json.load(f)['sessions']['$sess_name']
print(f'FIXTURE={s[\"fixture\"]}')
print(f'DESC=\"{s[\"description\"]}\"')
print(f'SETUP_CMD=\"{s.get(\"setup\", \"\")}\"')
print(f'NUM_TURNS={len(s[\"turns\"])}')
")"

  FIXTURE_SCRIPT="$SCRIPT_DIR/fixtures/${FIXTURE}/setup.sh"
  WS="/tmp/mc-session-${sess_name}"
  CTR="mc-session-${sess_name}"

  echo "============================================"
  echo " Session: $sess_name"
  echo " $DESC"
  echo " Model: $MODEL | Turns: $NUM_TURNS"
  echo "============================================"

  # Setup
  bash "$FIXTURE_SCRIPT" "$WS" >/dev/null 2>&1
  [ -n "$SETUP_CMD" ] && (cd "$WS" && eval "$SETUP_CMD") 2>/dev/null

  # Write turns to batch file
  python3 -c "
import json
with open('$SCENARIOS') as f:
    turns = json.load(f)['sessions']['$sess_name']['turns']
with open('$WS/session_turns.txt', 'w') as f:
    for t in turns:
        f.write(t + '\n')
"

  # Create container
  sudo docker rm -f "$CTR" >/dev/null 2>&1 || true
  sudo docker run -d --name "$CTR" \
    -v "$MUSL_BIN:/usr/local/bin/magic-code:ro" \
    -v "$WS:/workspace" -w /workspace \
    --network host --entrypoint sleep "$DOCKER_IMAGE" 900 >/dev/null 2>&1
  sleep 1

  # Run session
  T0=$(date +%s)
  sudo docker exec "$CTR" sh -c "
    timeout $TIMEOUT magic-code \
      --base-url $BASE_URL \
      --api-key $API_KEY \
      --model $MODEL \
      --yes --ndjson \
      --batch /workspace/session_turns.txt 2>&1
  " 2>&1 | python3 -c "
import sys, json

turns = []
current = {'tools': [], 'in': 0, 'out': 0, 'iters': 0}

for line in sys.stdin:
    line = line.strip()
    if not line: continue
    try:
        d = json.loads(line)
        t = d.get('type','')
        if t == 'turn_start':
            current = {'turn': d['turn'], 'prompt': d['prompt'], 'tools': [], 'in': 0, 'out': 0, 'iters': 0}
        elif t == 'tool_call':
            current['tools'].append(d['name'])
        elif t == 'turn_end':
            current['in'] = d.get('input_tokens', 0)
            current['out'] = d.get('output_tokens', 0)
            current['iters'] = d.get('iterations', 0)
            turns.append(current)
            tools_str = ','.join(current['tools']) if current['tools'] else '(none)'
            print(f'  T{current[\"turn\"]:>2} {current[\"in\"]:>6}in {current[\"out\"]:>4}out  {tools_str[:60]}')
    except:
        if line.startswith('[batch]'):
            print(f'  {line}')

# Write results
with open('$RESULT_FILE', 'w') as f:
    for t in turns:
        json.dump({
            'session': '$sess_name',
            'model': '$MODEL',
            'turn': t.get('turn', 0),
            'prompt': t.get('prompt', ''),
            'tools': t['tools'],
            'input_tokens': t['in'],
            'output_tokens': t['out'],
            'iterations': t['iters'],
            'timestamp': '$TIMESTAMP',
        }, f)
        f.write('\n')
"
  T1=$(date +%s)
  DUR=$((T1 - T0))

  # Verification
  VERIFY_JSON=$(python3 -c "
import json
with open('$SCENARIOS') as f:
    v = json.load(f)['sessions']['$sess_name'].get('verify', {})
print(json.dumps(v))
")
  VRESULT=$(python3 "$SCRIPT_DIR/verify.py" "$WS" "$VERIFY_JSON" 2>/dev/null) || VRESULT='{"all_pass":false,"details":["error"]}'
  VPASS=$(echo "$VRESULT" | python3 -c "import sys,json; print('✅' if json.load(sys.stdin).get('all_pass') else '❌')" 2>/dev/null)
  VDETAILS=$(echo "$VRESULT" | python3 -c "import sys,json; d=json.load(sys.stdin).get('details',[]); [print(f'    ↳ {x}') for x in d]" 2>/dev/null)

  echo ""
  echo "  Duration: ${DUR}s | Verify: $VPASS"
  [ -n "$VDETAILS" ] && echo "$VDETAILS"
  echo ""

  # Cleanup
  sudo docker rm -f "$CTR" >/dev/null 2>&1 || true
  rm -rf "$WS"
done
