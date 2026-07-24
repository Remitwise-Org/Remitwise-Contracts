#!/usr/bin/env python3
"""Check workspace-level invariants via grep-based analysis.

Issue #1095: Add a check-workspace-invariants CI job

Checks performed:
  1. Every workspace crate directory has a README.md
  2. Every public entrypoint (pub fn inside #[contractimpl]) has a doc comment (///)
  3. Every #[contracterror] variant has a doc comment (///)
  4. No bare `todo!()` or `unimplemented!()` in production (non-test) source files

Usage:
    python scripts/check_workspace_invariants.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import List, Tuple


def find_workspace_root() -> Path:
    script_dir = Path(__file__).resolve().parent
    for candidate in (script_dir, script_dir.parent):
        if (candidate / "Cargo.toml").exists():
            return candidate
    raise FileNotFoundError("Unable to locate workspace root")


def read_workspace_members(workspace_root: Path) -> List[str]:
    cargo_toml = workspace_root / "Cargo.toml"
    if not cargo_toml.exists():
        return []
    content = cargo_toml.read_text(encoding="utf-8")
    members: List[str] = []
    in_members = False
    for line in content.splitlines():
        stripped = line.strip()
        if stripped == "members = [":
            in_members = True
            continue
        if in_members:
            if stripped == "]":
                break
            m = re.match(r'"([^"]+)"', stripped)
            if m:
                members.append(m.group(1))
    return members


def is_test_file(path: Path) -> bool:
    name = path.name
    return "test" in name or name.endswith("_tests.rs") or name.startswith("tests_")


def is_production_rs(path: Path) -> bool:
    if path.suffix != ".rs":
        return False
    if is_test_file(path):
        return False
    parts = path.parts
    if "tests" in parts:
        return False
    return True


def extract_braced_block(text: str, start: int) -> Tuple[str, int]:
    brace = text.find("{", start)
    if brace == -1:
        return "", start
    depth = 0
    for i in range(brace, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[brace + 1 : i], i + 1
    return "", start


# ---------------------------------------------------------------------------
# Check 1: every workspace crate has a README
# ---------------------------------------------------------------------------

def check_readmes(workspace_root: Path, members: List[str]) -> List[str]:
    errors: List[str] = []
    for member in members:
        readme = workspace_root / member / "README.md"
        if not readme.exists():
            errors.append(f"Missing README.md in crate '{member}'")
    return errors


# ---------------------------------------------------------------------------
# Check 2: every pub fn inside #[contractimpl] has a doc comment
# ---------------------------------------------------------------------------

def check_entrypoint_docs(workspace_root: Path, members: List[str]) -> List[str]:
    errors: List[str] = []
    for member in members:
        src_dir = workspace_root / member / "src"
        if not src_dir.exists():
            continue
        for rs_file in src_dir.rglob("*.rs"):
            if not is_production_rs(rs_file):
                continue
            content = rs_file.read_text(encoding="utf-8")
            for m in re.finditer(r"#\[contractimpl(?:[^\]]*)\]\s*", content):
                impl_body, _ = extract_braced_block(content, m.end())
                if not impl_body:
                    continue
                # Find all pub fn definitions and check for preceding doc comment
                for fn_match in re.finditer(r"(?m)^\s*pub\s+fn\s+([A-Za-z0-9_]+)\s*\(", impl_body):
                    fn_name = fn_match.group(1)
                    fn_start_in_block = fn_match.start()
                    # Look backwards for /// on the lines immediately before the fn
                    prefix = impl_body[:fn_start_in_block]
                    lines_before = prefix.splitlines()
                    has_doc = False
                    for line in reversed(lines_before):
                        stripped = line.strip()
                        if not stripped:
                            continue
                        if stripped.startswith("///"):
                            has_doc = True
                            break
                        if stripped.startswith("#[") or stripped.startswith("//"):
                            continue
                        break
                    if not has_doc:
                        errors.append(
                            f"{member}/src/{rs_file.name}: entrypoint '{fn_name}' missing doc comment (///)"
                        )
    return errors


# ---------------------------------------------------------------------------
# Check 3: every #[contracterror] variant has a doc comment
# ---------------------------------------------------------------------------

def check_error_variant_docs(workspace_root: Path, members: List[str]) -> List[str]:
    errors: List[str] = []
    for member in members:
        src_dir = workspace_root / member / "src"
        if not src_dir.exists():
            continue
        for rs_file in src_dir.rglob("*.rs"):
            if not is_production_rs(rs_file):
                continue
            content = rs_file.read_text(encoding="utf-8")
            # Find #[contracterror] ... enum ... { ... }
            for err_match in re.finditer(r"#\[contracterror\].*?enum\s+\w+\s*\{", content, re.DOTALL):
                enum_body, _ = extract_braced_block(content, err_match.end() - 1)
                if not enum_body:
                    continue
                for variant_match in re.finditer(r"(?m)^\s*([A-Za-z0-9_]+)\s*=\s*\d+", enum_body):
                    variant_name = variant_match.group(1)
                    variant_start = variant_match.start()
                    prefix = enum_body[:variant_start]
                    lines_before = prefix.splitlines()
                    has_doc = False
                    for line in reversed(lines_before):
                        stripped = line.strip()
                        if not stripped:
                            continue
                        if stripped.startswith("///"):
                            has_doc = True
                            break
                        if stripped.startswith("#[") or stripped.startswith("//"):
                            continue
                        break
                    if not has_doc:
                        errors.append(
                            f"{member}/src/{rs_file.name}: contracterror variant '{variant_name}' missing doc comment (///)"
                        )
    return errors


# ---------------------------------------------------------------------------
# Check 4: no bare todo!() / unimplemented!() in production code
# ---------------------------------------------------------------------------

def check_no_todos(workspace_root: Path, members: List[str]) -> List[str]:
    errors: List[str] = []
    for member in members:
        src_dir = workspace_root / member / "src"
        if not src_dir.exists():
            continue
        for rs_file in src_dir.rglob("*.rs"):
            if not is_production_rs(rs_file):
                continue
            content = rs_file.read_text(encoding="utf-8")
            for lineno, line in enumerate(content.splitlines(), 1):
                stripped = line.strip()
                if re.search(r"\btodo!\(", stripped, re.IGNORECASE):
                    errors.append(
                        f"{member}/src/{rs_file.name}:{lineno}: bare `todo!()` in production code"
                    )
                if re.search(r"\bunimplemented!\(", stripped, re.IGNORECASE):
                    errors.append(
                        f"{member}/src/{rs_file.name}:{lineno}: bare `unimplemented!()` in production code"
                    )
    return errors


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> int:
    workspace_root = find_workspace_root()
    members = read_workspace_members(workspace_root)

    if not members:
        print("No workspace members found — nothing to check.")
        return 0

    all_errors: List[str] = []

    print("=== Check 1: every crate has README.md ===")
    errors = check_readmes(workspace_root, members)
    all_errors.extend(errors)
    if errors:
        for e in errors:
            print(f"  FAIL: {e}")
    else:
        print(f"  PASS: all {len(members)} crates have README.md")

    print("\n=== Check 2: every entrypoint has doc comment ===")
    errors = check_entrypoint_docs(workspace_root, members)
    all_errors.extend(errors)
    if errors:
        for e in errors:
            print(f"  FAIL: {e}")
    else:
        print("  PASS: all entrypoints have doc comments")

    print("\n=== Check 3: every contracterror variant has doc comment ===")
    errors = check_error_variant_docs(workspace_root, members)
    all_errors.extend(errors)
    if errors:
        for e in errors:
            print(f"  FAIL: {e}")
    else:
        print("  PASS: all contracterror variants have doc comments")

    print("\n=== Check 4: no bare todo!()/unimplemented!() in production code ===")
    errors = check_no_todos(workspace_root, members)
    all_errors.extend(errors)
    if errors:
        for e in errors:
            print(f"  FAIL: {e}")
    else:
        print("  PASS: no bare todo!()/unimplemented!() found")

    print(f"\n{'='*60}")
    if all_errors:
        print(f"FAILED: {len(all_errors)} invariant violation(s)")
        return 1
    else:
        print("PASSED: all workspace invariants satisfied")
        return 0


if __name__ == "__main__":
    sys.exit(main())
