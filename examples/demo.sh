#!/bin/bash
# Record a demo with: asciinema rec demo.cast
# Convert to GIF with: agg demo.cast demo.gif
# Or use: svg-term --in demo.cast --out demo.svg

echo "=== magic-code demo ==="
echo ""
echo "# 1. Quick fix"
echo '$ magic-code "fix the typo in src/main.rs"'
echo ""
sleep 1

echo "# 2. Self-hosted (zero cost)"
echo '$ magic-code --provider ollama --model qwen3.5:9b "add error handling"'
echo ""
sleep 1

echo "# 3. CI/CD integration"
echo '$ magic-code --yes --json "fix failing tests" -o result.json'
echo ""
sleep 1

echo "# 4. Batch processing"
echo '$ magic-code --yes --batch tasks.txt'
echo ""
sleep 1

echo "# 5. NDJSON streaming"
echo '$ magic-code --ndjson "explain auth.rs" | jq .type'
echo ""

echo "=== Install ==="
echo "curl -fsSL https://raw.githubusercontent.com/kienbui1995/mc-code/main/install.sh | sh"
