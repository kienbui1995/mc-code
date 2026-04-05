#!/bin/bash
# Demo script for asciinema recording
# Usage: asciinema rec demo.cast -c ./scripts/demo.sh

set -e
MC="./target/release/magic-code"

echo "# magic-code demo"
echo ""
sleep 1

echo '$ magic-code "Read Cargo.toml and tell me the version"'
sleep 0.5
$MC --provider litellm --base-url "${MC_BASE_URL}" --api-key "${MC_API_KEY}" --model "${MC_MODEL:-gemini/gemini-2.5-flash}" "Read Cargo.toml and tell me the version. Be brief."
echo ""
sleep 2

echo '$ magic-code "Create a hello.py that prints hello world"'
sleep 0.5
$MC --provider litellm --base-url "${MC_BASE_URL}" --api-key "${MC_API_KEY}" --model "${MC_MODEL:-gemini/gemini-2.5-flash}" "Create /tmp/mc-demo-hello.py that prints hello world. Then run it."
echo ""
sleep 2

echo '$ cat /tmp/mc-demo-hello.py'
cat /tmp/mc-demo-hello.py
echo ""
sleep 1

echo "# Done! magic-code — AI coding agent in your terminal"
