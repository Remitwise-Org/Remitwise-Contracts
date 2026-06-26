#!/usr/bin/env python3
"""Print Soroban contract entrypoints and their auth model.

The script scans workspace crates for public methods inside `#[contractimpl]`
blocks and reports whether each entrypoint uses Soroban auth primitives or is
left unauthenticated.

Usage:
    python scripts/entrypoint_auth_inventory.py
    python scripts/entrypoint_auth_inventory.py remittance_split
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Sequence


@dataclass(frozen=True)
class Entrypoint:
    crate_name: str
    function_name: str
    auth_model: str
    source_file: str


def find_workspace_root(start: Optional[Path] = None) -> Path:
    base = (start or Path(__file__)).resolve().parent
    for candidate in [base, base.parent]:
        if (candidate / "Cargo.toml").exists():
            return candidate
    raise FileNotFoundError("Unable to locate workspace root from script location")


def iter_workspace_member_dirs(workspace_root: Path) -> List[Path]:
    cargo_toml = workspace_root / "Cargo.toml"
    if not cargo_toml.exists():
        return []

    members: List[Path] = []
    in_members = False
    for line in cargo_toml.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped == "members = [":
            in_members = True
            continue
        if in_members:
            if stripped == "]":
                break
            match = re.match(r'"([^"]+)"', stripped)
            if match:
                members.append(workspace_root / match.group(1))
    return members


def extract_block(text: str, start_index: int) -> Optional[tuple[str, int]]:
    brace_index = text.find("{", start_index)
    if brace_index == -1:
        return None

    depth = 0
    for index in range(brace_index, len(text)):
        char = text[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[brace_index : index + 1], index + 1
    return None


def find_entrypoints_in_file(path: Path, crate_name: str) -> List[Entrypoint]:
    content = path.read_text(encoding="utf-8")
    entrypoints: List[Entrypoint] = []
    for match in re.finditer(r"#\[contractimpl(?:[^\]]*)\]\s*", content):
        impl_start = match.end()
        impl_block = extract_block(content, impl_start)
        if not impl_block:
            continue
        impl_body, _ = impl_block
        for method_match in re.finditer(r"(?m)^\s*pub\s+fn\s+([A-Za-z0-9_]+)\s*\(", impl_body):
            function_name = method_match.group(1)
            method_start = method_match.end()
            method_block = extract_block(impl_body, method_start)
            if not method_block:
                continue
            body_text, _ = method_block
            auth_model = classify_auth_model(body_text)
            entrypoints.append(
                Entrypoint(
                    crate_name=crate_name,
                    function_name=function_name,
                    auth_model=auth_model,
                    source_file=str(path),
                )
            )
    return entrypoints


def classify_auth_model(method_body: str) -> str:
    auth_call = re.search(r"([A-Za-z0-9_$.]+)\.require_auth(?:_for_args)?\s*\(", method_body)
    if auth_call:
        return f"authenticated ({auth_call.group(1)}.require_auth)"

    method_name = re.search(r"fn\s+([A-Za-z0-9_]+)\s*\(", method_body)
    if method_name and is_read_only_name(method_name.group(1)):
        return "none (read-only)"

    return "none"


def is_read_only_name(name: str) -> bool:
    prefixes = ("get_", "is_", "check_", "calculate_", "verify_", "compute_", "list_", "fetch_")
    return name.startswith(prefixes)


def collect_entrypoints(workspace_root: Path, crate_filter: Optional[str] = None) -> List[Entrypoint]:
    entrypoints: List[Entrypoint] = []
    for member_dir in iter_workspace_member_dirs(workspace_root):
        crate_name = member_dir.name
        if crate_filter and crate_name != crate_filter:
            continue
        src_dir = member_dir / "src"
        if not src_dir.exists():
            continue
        for path in sorted(src_dir.rglob("*.rs")):
            if path.name.endswith(".rs"):
                entrypoints.extend(find_entrypoints_in_file(path, crate_name))
    return sorted(entrypoints, key=lambda item: (item.crate_name, item.function_name))


def render_entrypoints(entrypoints: Sequence[Entrypoint]) -> str:
    if not entrypoints:
        return "No contract entrypoints found."

    lines: List[str] = []
    current_crate: Optional[str] = None
    for entrypoint in entrypoints:
        if entrypoint.crate_name != current_crate:
            current_crate = entrypoint.crate_name
            lines.append(f"{current_crate}:")
        lines.append(f"  - {entrypoint.function_name}: {entrypoint.auth_model}")
    return "\n".join(lines)


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("crate", nargs="?", help="Limit output to a single workspace crate")
    args = parser.parse_args(argv)

    workspace_root = find_workspace_root()
    entrypoints = collect_entrypoints(workspace_root, crate_filter=args.crate)
    print(render_entrypoints(entrypoints))
    return 0


if __name__ == "__main__":
    sys.exit(main())
