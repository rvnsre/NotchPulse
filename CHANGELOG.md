# Changelog

## Unreleased

- Correct Rust/WASM IPC serialization: Tauri receives plain JSON objects, not JavaScript Maps.
- Catch synchronous desktop-bridge failures and surface actionable UI errors.
- Reject stale snapshot and recipe responses; preserve the selected repository across feed reorderings.
- Bind terminal execution to the exact command that was reviewed.
- Ignore superseded native resize requests and recover from failed expansion.
- Defer manual refresh until its cooldown expires instead of losing the request.
- Use monotonic clocks for request budgets and terminal launch throttling.
- Prevent old personalization responses and startup scans from replacing newer preferences.
- Keep filesystem synchronization off the UI thread and avoid holding the cache lock during HTTP requests.
- Write through exclusive temporary files, atomically replace saved data, and bound actual reads and writes.
- Resolve GUI-launched Homebrew/Cargo environments and execute explicit program paths.
- Preserve empty shell arguments; verify Unix executable permissions.
- Honor committed dependency lockfiles and set Python 3.12 explicitly in CI.
- Add browser regression tests for IPC and escaping, plus native scheduler and persistence tests.
- Reject missing, empty, stale or conflicting release outputs before staging.
- Prepare guarded private GitHub publication without force-pushing or overwriting an unrelated origin.
