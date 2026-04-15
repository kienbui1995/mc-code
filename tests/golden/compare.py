#!/usr/bin/env python3
"""Compare golden test results across runs/models.

Usage:
    ./compare.py results/model-a.jsonl results/model-b.jsonl
    ./compare.py results/  # compare all files in directory
"""
import json, sys, os
from collections import defaultdict

def load_results(path):
    results = {}
    with open(path) as f:
        for line in f:
            r = json.loads(line)
            results[r['id']] = r
    return results

def compare(files):
    all_results = {}
    for f in files:
        name = os.path.basename(f).replace('.jsonl', '')
        all_results[name] = load_results(f)

    # Collect all scenario IDs
    all_ids = sorted(set(id for r in all_results.values() for id in r))

    # Header
    names = list(all_results.keys())
    header = f"{'ID':<8} {'Category':<18}"
    for n in names:
        header += f" | {n[:20]:<20}"
    print(header)
    print("-" * len(header))

    # Rows
    for sid in all_ids:
        cat = ""
        row = f"{sid:<8}"
        for n in names:
            r = all_results[n].get(sid)
            if r:
                cat = r['category']
                tools = len(r['tools']) if r['tools'] and r['tools'] != [''] else 0
                tok = r['input_tokens']
                dur = r['duration_sec']
                status = "✅" if r['has_output'] else "❌"
                row += f" | {status} {tools}t {tok:>5}tk {dur:>3}s  "
            else:
                row += f" | {'—':<20}"
        print(f"{row[:8]} {cat:<18}{row[8:]}")

    # Summary per model
    print("\n=== Summary ===")
    for n in names:
        rs = list(all_results[n].values())
        total = len(rs)
        ok = sum(1 for r in rs if r['has_output'])
        tools = sum(1 for r in rs if r['tools'] and r['tools'] != [''])
        avg_tok = sum(r['input_tokens'] for r in rs) / max(total, 1)
        avg_dur = sum(r['duration_sec'] for r in rs) / max(total, 1)
        print(f"  {n}: {ok}/{total} output, {tools}/{total} tools, avg {avg_tok:.0f}tok {avg_dur:.1f}s")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    path = sys.argv[1]
    if os.path.isdir(path):
        files = sorted(f"{path}/{f}" for f in os.listdir(path) if f.endswith('.jsonl'))
    else:
        files = sys.argv[1:]

    if not files:
        print("No .jsonl files found")
        sys.exit(1)

    compare(files)
