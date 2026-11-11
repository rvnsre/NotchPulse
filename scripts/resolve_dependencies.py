"""Honor a committed lockfile; bootstrap only for a new source delivery."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
if (root / "Cargo.lock").is_file():
    subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=root, check=True, stdout=subprocess.DEVNULL)
else:
    subprocess.run(["cargo", "generate-lockfile"], cwd=root, check=True)
