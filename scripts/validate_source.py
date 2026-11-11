"""Dependency-free structural checks. These do not replace cargo or desktop QA."""
from pathlib import Path
import json
import hashlib
import subprocess
import sys
import tempfile
import tomllib
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parents[1]
checks = 0


def check(condition, message):
    global checks
    if not condition:
        raise AssertionError(message)
    checks += 1


for path in root.rglob("*.toml"):
    if "target" not in path.parts:
        tomllib.loads(path.read_text(encoding="utf-8"))
        checks += 1
for path in root.rglob("*.json"):
    if "target" not in path.parts:
        json.loads(path.read_text(encoding="utf-8"))
        checks += 1
ET.parse(root / "docs/hero.svg")
checks += 1

workspace = tomllib.loads((root / "Cargo.toml").read_text())
for member in workspace["workspace"]["members"]:
    check((root / member / "Cargo.toml").is_file(), f"Workspace member missing: {member}")
    check((root / member / "src").is_dir(), f"Workspace source missing: {member}")

config = json.loads((root / "src-tauri/tauri.conf.json").read_text())
check(config["version"] == workspace["workspace"]["package"]["version"], "Version mismatch")
window = config["app"]["windows"][0]
check(window["transparent"] and window["alwaysOnTop"] and not window["decorations"], "Window requirements missing")
check("unsafe-eval" not in config["app"]["security"]["csp"].replace("wasm-unsafe-eval", ""), "Generic eval enabled")
for icon in config["bundle"]["icon"]:
    check((root / "src-tauri" / icon).is_file(), f"Icon missing: {icon}")

ui = (root / "crates/pulse-ui/src/lib.rs").read_text(encoding="utf-8")
backend = (root / "src-tauri/src/main.rs").read_text(encoding="utf-8")
commands = ["snapshot", "command_for", "copy_command", "execute_command", "open_repository", "refresh_feed", "save_settings", "resize_island", "quit_app"]
for command in commands:
    check(f"fn {command}(" in backend, f"Missing IPC command: {command}")
    check(f'"{command}"' in ui, f"Unused IPC command: {command}")
check("set_interval" not in ui, "Unexpected frontend interval loop")
check("escape(&repo.full_name)" in ui, "Repository name not escaped")
check(not list((root / "crates").rglob("*.js")), "Handwritten JavaScript introduced")

for workflow in (root / ".github/workflows").glob("*.yml"):
    text = workflow.read_text(encoding="utf-8")
    check("permissions:" in text, f"Explicit workflow permissions missing: {workflow.name}")
    check("pull_request_target" not in text, "Unsafe PR execution context")

version = workspace["workspace"]["package"]["version"]
subprocess.run([sys.executable, str(root / "scripts/check_version.py"), f"v{version}"], check=True)
bad = subprocess.run([sys.executable, str(root / "scripts/check_version.py"), "v9.9.9"], capture_output=True)
check(bad.returncode != 0, "Version guard accepted a bad tag")
optimized = subprocess.run([sys.executable, "-O", str(root / "scripts/check_version.py"), "v9.9.9"], capture_output=True)
check(optimized.returncode != 0, "Python optimization disabled the version guard")

# Exercise staging against synthetic binary fixtures in isolation. Nothing here
# is represented as a compiled application artifact.
with tempfile.TemporaryDirectory(prefix="notchpulse-stage-") as temp:
    temp = Path(temp)
    scripts = temp / "scripts"
    scripts.mkdir()
    script = scripts / "stage_release.py"
    script.write_bytes((root / "scripts/stage_release.py").read_bytes())
    target = "x86_64-pc-windows-msvc"
    release = temp / "target" / target / "release"
    installer = release / "bundle/nsis/test-setup.exe"
    installer.parent.mkdir(parents=True)
    installer.write_bytes(b"synthetic installer fixture")
    (release / "notchpulse.exe").write_bytes(b"synthetic EXE fixture")
    subprocess.run([sys.executable, str(script), target, "windows-x64"], check=True)
    assets = temp / "release-assets/windows-x64"
    check((assets / "NotchPulse-windows-x64.exe").is_file(), "Standalone EXE staging failed")
    check(len((assets / "SHA256SUMS-windows-x64.txt").read_text().splitlines()) == 2, "Checksum staging failed")
    (release / "notchpulse.exe").unlink()
    missing = subprocess.run([sys.executable, str(script), target, "windows-x64"], capture_output=True)
    check(missing.returncode != 0, "Missing EXE did not fail staging")
    check((assets / "NotchPulse-windows-x64.exe").read_bytes() == b"synthetic EXE fixture", "Failed staging changed previous output")
    wrong = subprocess.run([sys.executable, str(script), "../../outside", "windows-x64"], capture_output=True)
    check(wrong.returncode != 0, "Staging accepted an invalid target path")

    for target, platform, suffix in [
        ("universal-apple-darwin", "macos-universal", ".dmg"),
        ("x86_64-unknown-linux-gnu", "linux-x64", ".rpm"),
    ]:
        fixture = temp / "target" / target / "release/bundle/test" / f"installer{suffix}"
        fixture.parent.mkdir(parents=True)
        fixture.write_bytes(b"synthetic native packaging fixture")
        subprocess.run([sys.executable, str(script), target, platform], check=True)
        checksum = (temp / "release-assets" / platform / f"SHA256SUMS-{platform}.txt").read_text()
        check(hashlib.sha256(fixture.read_bytes()).hexdigest() in checksum, f"Wrong checksum for {platform}")
        stale = subprocess.run([sys.executable, str(script), target, platform], capture_output=True)
        check(stale.returncode != 0, f"Stale staging accepted for {platform}")

print(f"PASS: {checks} structural and release-script checks (not Rust compilation)")
