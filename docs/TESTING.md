# Validation and acceptance

## Authoring environment

The source was authored on Windows without cargo/rustc. Network policy prevented downloading the toolchain. JSON/TOML parsing, XML assets, repository structure, release-script behavior and source invariants can be checked locally with `python scripts/validate_source.py`. This is a source check, not a Rust compilation or desktop test. Do not equate it with the native CI jobs below.

## Automated Rust checks

```sh
cargo fmt --all
cargo test -p pulse-core
cargo clippy -p pulse-ui --target wasm32-unknown-unknown -- -D warnings
trunk build --release
cargo clippy -p notchpulse --all-targets -- -D warnings
cargo test -p notchpulse
```

Core tests cover invalid repository names, star threshold, ranking, deduplication, exact package mappings, shell quoting, mixed-DPI/negative-origin anchoring, monitor clamping, nonfinite preferences, and spring settling/reversal. Native tests cover bounded preference reads, replacement, and shallow manifest scanning. CI runs on all three desktop platforms. Packaging must also pass before a release is described as build verified.

Additional regression coverage includes deferred manual refresh, server cooldowns, backoff reset, preserving saved data after an oversized write, and browser tests that inspect the actual JavaScript type of IPC arguments. Linux CI runs the WASM tests in headless Chrome through wasm-pack. These tests have been added to the source; they are not represented as locally executed Rust tests.

## Manual acceptance matrix

| Scenario | Required observation |
| --- | --- |
| Fresh launch, online | Compact pill is anchored to primary display; the first fetch yields only eligible cards |
| Fresh launch, offline | Honest empty state with retry; no fabricated repository cards |
| Warm offline launch | Last saved cards remain readable and status indicates cached/offline data |
| API 403 / 429 | No retry loop; reset/Retry-After observed; pause remains responsive |
| Corrupt cache or preferences | Bounded parser falls back safely; app still starts |
| Hover reversal | Spring reverses without a jump; no stuck expanded native rectangle |
| Collapse | Click a desktop item directly below the pill; expanded rectangle must no longer intercept it |
| Keyboard only | Focus pill, use arrows, open settings/review, Escape, copy and navigate with visible focus |
| Reduced motion | Native and app preferences skip the spring; controls remain available |
| Incoming update during review | Reviewed command stays fixed; vanished backend ID returns an error |
| Preferences validation | NaN, out-of-range offsets and invalid folders are rejected without losing entered text |
| Mixed DPI / negative origins | Test 100%, 150%, 200% monitors, primary switching, unplug and replug |
| Notched Mac | Primary safe-area inset observed; Retina coordinates remain correct |
| External webcam | Manual x offset lands under camera; chosen display falls back when missing |
| Windows terminal | Test winget present/absent and WT present/absent; PowerShell stays visible |
| macOS terminal | Intel and Apple Silicon use native shell and correctly initialized Homebrew |
| Linux launchers | Test GNOME Terminal, Konsole, Xfce, xterm, Kitty and no supported terminal |
| Linux Wayland | Placement limitation is visible; no universal-positioning claim |
| Suspicious metadata | HTML entities and quotes remain text; no code from README/API is executed |
| Tray and duplicate process | Show/Refresh/Quit work; second launch activates first instance |
| Suspend/resume | No burst of queued refreshes or catch-up animation frames |
| Install/uninstall | NSIS clean-user test, universal DMG architectures, RPM dependency resolution on intended target |

No checked boxes are pre-populated: these need observations on real target desktops.
