# Agents Station

Native Rust 3D multi-agent office & GUI harness launcher — a zero-cut, low-RAM rebuild of [`my-station`](https://github.com/BoyinGlasses/my-station) powered by **Bevy (`wgpu`)** + **`bevy_egui`** + **Axum/Tokio** + native **Win32** APIs.

## Why Agents Station?
- **Extreme RAM reduction on 8 GB hosts:** replaces Chromium/WebGL (~2.2–2.4 GB RAM) and Node/Next.js + periodic Windows PowerShell spikes (+70–123 MB) with a single native binary:
  - System Tray background daemon: **≤ 45 MB Private** (0 PowerShell spawns)
  - 2D Lite / Machine Panel: **≤ 110 MB Private**
  - Full 3D Pro / HQ Office: **≤ 300–380 MB Private**
- **100% Feature Parity:** full drop-in compatibility with `my-station` port `4310`, `data/` schemas, hooks (`claude`, `codex`, `opencode`, `zcode`, `antigravity`), OTLP telemetry, `/phone` PWA + Web Push, 3 layouts (`cozy`, `pro`, `hq`), 3 disposable backdrops (`city`, `countryside`, `beach`), chibi rig, street life, 42 UI screens, and 105 i18n locales.
- **Native GUI Harness Launcher:** auto-detects installed desktop harness apps (`Claude`, `Codex`, `OpenCode`, `ZCode`, `Antigravity`, plus user-defined custom apps), ensures hooks are wired, and focuses or launches the GUI app directly without spawning a terminal.

## Planning & Architecture Documents
- [Design Spec & 100% Parity Ledger](docs/superpowers/specs/2026-11-25-agents-station-design.md)
- [Plan 0 — Workspace Bootstrap, 8 GB Guard & Screenshot Harness](docs/superpowers/plans/2026-11-25-agents-station-p0-bootstrap.md)
- [Plan 1 — Full Backend Parity (`station-core`) & Native GUI Harness Launcher (`station-launcher`)](docs/superpowers/plans/2026-11-25-agents-station-p1-core-and-launcher.md)

## Build Guardrails (8 GB RAM / Windows Host)
`.cargo/config.toml` pins `target-dir = "D:/mystation/cargo-target"` and `jobs = 2` to prevent OOM and avoid filling `C:`. Never invoke two concurrent `cargo` builds on an 8 GB host.
