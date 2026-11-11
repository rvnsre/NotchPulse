"""Fail closed if any required packaging output is missing."""
import hashlib
from pathlib import Path
import shutil
import sys

root = Path(__file__).resolve().parents[1]
if len(sys.argv) != 3:
    raise SystemExit("Usage: stage_release.py TARGET PLATFORM")
target, platform = sys.argv[1:]
targets = {"windows-x64": ("x86_64-pc-windows-msvc", ".exe"), "macos-universal": ("universal-apple-darwin", ".dmg"), "linux-x64": ("x86_64-unknown-linux-gnu", ".rpm")}
if platform not in targets or target != targets[platform][0]:
    raise SystemExit("Unsupported target/platform pairing")
base = root / "target" / target / "release"
out = root / "release-assets" / platform
suffix = targets[platform][1]
installers = sorted((base / "bundle").rglob(f"*{suffix}"))
if not installers:
    raise SystemExit(f"No {suffix} installer produced")
sources = [(p, p.name) for p in installers]
if platform == "windows-x64":
    exe = base / "notchpulse.exe"
    sources.append((exe, "NotchPulse-windows-x64.exe"))
if any(not p.is_file() or p.stat().st_size == 0 for p, _ in sources):
    raise SystemExit("A required installer or standalone EXE is missing or empty")
if len({name for _, name in sources}) != len(sources):
    raise SystemExit("Artifact filename collision")
if out.exists():
    raise SystemExit("Staging directory already exists; refusing to mix stale release assets")
out.mkdir(parents=True)
lines = []
for path, name in sources:
    copied = out / name
    shutil.copy2(path, copied)
    with copied.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    lines.append(f"{digest}  {name}")
(out / f"SHA256SUMS-{platform}.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"Staged {len(lines)} artifacts for {platform}")

