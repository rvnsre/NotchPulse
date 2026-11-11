"""Reject release tags which do not match all application versions."""
import json
from pathlib import Path
import sys
import tomllib

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
config = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
if config["version"] != version:
    raise SystemExit("Tauri and workspace versions must match")
if len(sys.argv) != 2 or sys.argv[1] != f"v{version}":
    raise SystemExit(f"Expected exactly one release tag: v{version}")
print(f"Release version verified: {version}")

