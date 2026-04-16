#!/usr/bin/env python3
"""Verify golden test results against scenario expectations.
Runs inside Docker container after mc-code completes.

Usage: verify.py <workspace_dir> <scenario_json>
Returns JSON: {"l0": bool, "l1": bool, "l2": bool, "details": [...]}
"""
import json, sys, os, subprocess

def check_file_exists(ws, path):
    full = os.path.join(ws, path)
    return os.path.isfile(full), f"file_exists: {path}"

def check_file_contains(ws, path, patterns):
    full = os.path.join(ws, path)
    if not os.path.isfile(full):
        return False, f"file_contains: {path} not found"
    try:
        content = open(full).read().lower()
    except:
        return False, f"file_contains: {path} unreadable"
    missing = [p for p in patterns if p.lower() not in content]
    if missing:
        return False, f"file_contains: {path} missing [{', '.join(missing)}]"
    return True, f"file_contains: {path} OK"

def check_file_not_contains(ws, path, patterns):
    full = os.path.join(ws, path)
    if not os.path.isfile(full):
        return True, f"file_not_contains: {path} not found (OK)"
    content = open(full).read().lower()
    found = [p for p in patterns if p.lower() in content]
    if found:
        return False, f"file_not_contains: {path} still has [{', '.join(found)}]"
    return True, f"file_not_contains: {path} OK"

def verify(ws, verify_rules):
    if not verify_rules:
        return {"l0": True, "l1": None, "l2": None, "details": ["no verification rules"]}

    results = []
    all_pass = True

    # L1: file existence
    for path in verify_rules.get("file_exists", []):
        ok, msg = check_file_exists(ws, path)
        results.append({"check": "file_exists", "pass": ok, "msg": msg})
        if not ok:
            all_pass = False

    # L2: file content
    for path, patterns in verify_rules.get("file_contains", {}).items():
        ok, msg = check_file_contains(ws, path, patterns)
        results.append({"check": "file_contains", "pass": ok, "msg": msg})
        if not ok:
            all_pass = False

    for path, patterns in verify_rules.get("file_not_contains", {}).items():
        ok, msg = check_file_not_contains(ws, path, patterns)
        results.append({"check": "file_not_contains", "pass": ok, "msg": msg})
        if not ok:
            all_pass = False

    l1 = all(r["pass"] for r in results if r["check"] == "file_exists") if any(r["check"] == "file_exists" for r in results) else None
    l2 = all(r["pass"] for r in results if r["check"] in ("file_contains", "file_not_contains")) if any(r["check"] in ("file_contains", "file_not_contains") for r in results) else None

    return {
        "l0": True,
        "l1": l1,
        "l2": l2,
        "all_pass": all_pass,
        "details": [r["msg"] for r in results if not r["pass"]]
    }

if __name__ == "__main__":
    ws = sys.argv[1]
    rules = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    result = verify(ws, rules)
    print(json.dumps(result))
