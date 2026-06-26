#!/usr/bin/env python3
"""
Feature Flag Consistency Checker

Scans all workspace member crates for cfg(feature = "...") references in Rust
source files and verifies that each referenced feature is declared in the
crate's [features] section of Cargo.toml.

Usage:
    python scripts/check_features.py
"""

import os
import re
import sys

WORKSPACE_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def parse_workspace_members():
    path = os.path.join(WORKSPACE_ROOT, "Cargo.toml")
    with open(path) as f:
        content = f.read()

    members = []
    in_members = False
    for line in content.splitlines():
        stripped = line.strip()
        if stripped == "members = [":
            in_members = True
        elif in_members:
            if stripped == "]":
                break
            m = re.match(r'\s*"([^"]+)"', stripped)
            if m:
                members.append(m.group(1))
    return members


def parse_features(crate_dir):
    path = os.path.join(crate_dir, "Cargo.toml")
    if not os.path.exists(path):
        return set()

    with open(path) as f:
        content = f.read()

    features = set()
    in_features = False
    for line in content.splitlines():
        stripped = line.strip()
        if stripped == "[features]":
            in_features = True
            continue
        if in_features:
            if stripped.startswith("["):
                break
            if "=" in stripped and not stripped.startswith("#"):
                name = stripped.split("=")[0].strip()
                features.add(name)
    return features


def scan_feature_references(crate_dir):
    src_dir = os.path.join(crate_dir, "src")
    if not os.path.exists(src_dir):
        return set()

    pattern = re.compile(r'feature\s*=\s*"([^"]+)"')
    references = set()

    for root, dirs, files in os.walk(src_dir):
        for f in files:
            if not f.endswith(".rs"):
                continue
            path = os.path.join(root, f)
            try:
                with open(path, encoding="utf-8") as fh:
                    content = fh.read()
                for m in pattern.finditer(content):
                    references.add(m.group(1))
            except Exception as exc:
                print(f"  warning: skipping {path}: {exc}", file=sys.stderr)
                continue

    return references


def main():
    workspace_toml = os.path.join(WORKSPACE_ROOT, "Cargo.toml")
    print(f"workspace root: {WORKSPACE_ROOT}", flush=True)
    print(f"workspace Cargo.toml: {workspace_toml}", flush=True)

    if not os.path.isfile(workspace_toml):
        print(f"error: no Cargo.toml found at {workspace_toml}", file=sys.stderr)
        return 1

    members = parse_workspace_members()
    print(f"discovered {len(members)} workspace members", flush=True)

    violations = []

    for member in members:
        crate_dir = os.path.join(WORKSPACE_ROOT, member)
        if not os.path.isdir(crate_dir):
            print(f"  skipping {member}: directory not found", flush=True)
            continue

        declared = parse_features(crate_dir)
        referenced = scan_feature_references(crate_dir)

        print(
            f"  {member}: {len(declared)} features declared, "
            f"{len(referenced)} features referenced in source",
            flush=True,
        )

        for name in sorted(referenced):
            if name not in declared:
                crate_name = os.path.basename(crate_dir)
                violations.append(
                    f'{crate_name}: feature "{name}" is referenced in source '
                    f'but not declared in [features]'
                )

    if not violations:
        print("✓ All feature flags referenced in source are declared in Cargo.toml")
        return 0

    print("Error: feature flag inconsistencies found:", file=sys.stderr)
    for v in violations:
        print(f"  {v}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
