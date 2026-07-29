import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "entrypoint_auth_inventory.py"


def test_inventory_lists_known_entrypoints():
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "remittance_split"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )

    output = result.stdout
    assert "remittance_split:" in output
    assert "initialize_split" in output
    assert "get_split" in output
    assert "distribute_usdc" in output
