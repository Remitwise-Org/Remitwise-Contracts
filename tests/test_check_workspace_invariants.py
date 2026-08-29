import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT))

from scripts.check_workspace_invariants import (  # noqa: E402
    check_no_dead_crates,
    read_workspace_members,
)


def _write_cargo_toml(crate_dir: Path, deps: dict | None = None, dev_deps: dict | None = None) -> None:
    crate_dir.mkdir(parents=True, exist_ok=True)
    lines = ["[package]", f'name = "{crate_dir.name}"', 'version = "0.1.0"', ""]
    if deps:
        lines.append("[dependencies]")
        for name, rel_path in deps.items():
            lines.append(f'{name} = {{ path = "{rel_path}" }}')
        lines.append("")
    if dev_deps:
        lines.append("[dev-dependencies]")
        for name, rel_path in dev_deps.items():
            lines.append(f'{name} = {{ path = "{rel_path}" }}')
        lines.append("")
    (crate_dir / "Cargo.toml").write_text("\n".join(lines), encoding="utf-8")


def test_flags_crate_with_no_references_and_no_tests(tmp_path):
    members = ["used", "orphan", "tested_only"]
    for name in members:
        _write_cargo_toml(tmp_path / name)

    # Root package depends on "used", mirroring how this repo's root
    # Cargo.toml depends on most of the contract crates.
    _write_cargo_toml(tmp_path, deps={"used": "./used"})

    # "tested_only" is not referenced anywhere, but has its own tests.
    tests_dir = tmp_path / "tested_only" / "tests"
    tests_dir.mkdir(parents=True)
    (tests_dir / "smoke.rs").write_text("#[test]\nfn it_works() {}\n", encoding="utf-8")

    errors = check_no_dead_crates(tmp_path, members)

    assert len(errors) == 1
    assert "orphan" in errors[0]


def test_dev_dependency_reference_counts_as_used(tmp_path):
    members = ["lib_crate", "test_helpers"]
    _write_cargo_toml(tmp_path / "lib_crate", dev_deps={"test_helpers": "../test_helpers"})
    _write_cargo_toml(tmp_path / "test_helpers")
    # Root depends on "lib_crate" directly; "test_helpers" is only ever
    # reachable via lib_crate's [dev-dependencies], which must still count.
    _write_cargo_toml(tmp_path, deps={"lib_crate": "./lib_crate"})

    errors = check_no_dead_crates(tmp_path, members)

    assert errors == []


def test_no_false_positives_against_this_repo():
    """Regression check against the real workspace: today, exactly one crate
    ('cli') is neither depended on by another crate nor has its own tests.
    If this starts failing because a *different* crate regresses, that's a
    real bug this check is meant to catch."""
    members = read_workspace_members(REPO_ROOT)
    errors = check_no_dead_crates(REPO_ROOT, members)

    assert len(errors) == 1
    assert "'cli'" in errors[0]
