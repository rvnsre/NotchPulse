# Contributing

Keep application behavior in Rust. Prefer bounded work and native facilities over extra runtime processes. Any new installation recipe must map an exact upstream identity to a reviewed package and include platform tests. Never accept arbitrary shell text from IPC or repository metadata.

Run the commands in docs/TESTING.md, explain the concrete trigger and resulting behavior in your pull request, and include platform observations for display or terminal changes. Do not assert benchmark improvements without release-build measurements.

Resolve and commit the workspace lockfile before maintaining a public release branch. Format with rustfmt and keep compiler/clippy checks clean on supported hosts.
