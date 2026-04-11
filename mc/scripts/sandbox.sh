#!/bin/bash
# Usage: ./sandbox.sh [prompt...]
# Creates a temp sandbox dir, runs magic-code in it, cleans up on exit.

set -e
SANDBOX=$(mktemp -d /tmp/mc-sandbox-XXXXXX)
echo "🔒 Sandbox: $SANDBOX"

cd "$SANDBOX"
git init -q

# Copy project files if you want to test on real code:
# cp -r /path/to/project/* .

MC="${MC_BIN:-$HOME/magic-code/magic-code/mc/target/release/magic-code}"

"$MC" \
  --provider "${MC_PROVIDER:-litellm}" \
  --base-url "${MC_BASE_URL}" \
  --api-key "${MC_API_KEY}" \
  --model "${MC_MODEL:-gemini/gemini-2.5-flash}" \
  "$@"

echo ""
echo "📁 Sandbox files:"
find . -not -path './.git/*' -not -name '.git' -type f
echo ""
read -p "🗑️  Delete sandbox? [Y/n] " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Nn]$ ]]; then
  rm -rf "$SANDBOX"
  echo "Cleaned up."
else
  echo "Kept at: $SANDBOX"
fi
