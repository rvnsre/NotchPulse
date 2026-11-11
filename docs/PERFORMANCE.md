# Performance measurement protocol

Do not publish a memory or latency badge before collecting measurements from a release build on each supported platform. “Rust” alone does not bound a webview's process tree.

## Repeatable scenario

1. Build release artifacts with a retained `Cargo.lock`. Record commit, compiler, OS version, CPU/GPU, display scaling and refresh rate.
2. Launch cold, wait for one successful feed request, then leave the island collapsed for five minutes.
3. Record median and p95 native-process memory plus **aggregate process-tree memory**. Record shared memory treatment explicitly.
4. Perform 100 hover expand/collapse cycles. Capture frame times and pointer-to-first-present latency. Repeat at 60 Hz and 120 Hz when available.
5. Remain idle for 30 minutes, including two feed refreshes. Record CPU time, network request count and bytes, and battery/energy metrics.
6. Repeat offline, after resume from sleep, under API throttling, with reduced motion, and with mixed-DPI displays.

## Tools

| Platform | Memory and CPU | Frame and energy measurements |
| --- | --- | --- |
| Windows | Process Explorer or Task Manager, include WebView2 children | Windows Performance Recorder / Analyzer, PresentMon |
| macOS | Activity Monitor and `footprint`, include WebKit helpers | Instruments Animation Hitches / Time Profiler / Energy Log |
| Linux X11 | `smem`, `/proc/*/smaps_rollup`, `pidstat` | compositor frame diagnostics, `powertop` |

## Budgets to evaluate

- Native process: investigate a **30 MiB idle target**. Keep this distinct from total application memory.
- Renderer: report measured values; no hard memory guarantee.
- Motion: one compositor frame per display refresh while animating, no perpetual RAF after settling.
- Idle frontend: no interval polling, remote fonts or images.
- Feed: 12 retained cards, 30-result request, 1 MiB response ceiling, 15–16-minute nominal cadence, 60-second manual minimum.

The CI compiles and tests logic; it does not establish these performance measurements. Keep benchmark results in a separate dated report with raw data.
