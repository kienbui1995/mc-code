#!/bin/bash
# Batch code review: review multiple files
# Usage: ./batch-review.sh src/*.rs

TMPFILE=$(mktemp)
for file in "$@"; do
    echo "Review $file for bugs, security issues, and improvements. Be specific about line numbers." >> "$TMPFILE"
done

magic-code --yes --json --batch "$TMPFILE"
rm "$TMPFILE"
