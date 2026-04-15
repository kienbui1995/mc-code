#!/bin/bash
# NDJSON streaming: pipe magic-code events to a web app
# Usage: ./stream-to-webapp.sh "your prompt"

magic-code --yes --ndjson "$1" | while IFS= read -r line; do
    type=$(echo "$line" | jq -r '.type // empty' 2>/dev/null)
    case "$type" in
        text)
            echo "$line" | jq -r '.content' ;;
        tool_call)
            echo "[TOOL] $(echo "$line" | jq -r '.name')" ;;
        tool_output)
            echo "[OUTPUT] $(echo "$line" | jq -r '.content' | head -5)" ;;
    esac
done
