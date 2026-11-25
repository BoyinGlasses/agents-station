# Agents Station — Plan 6: Full E2E, Visual Certification, RAM Soak & `--release` Shipping Gate

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans and superpowers:verification-before-completion under a single Cargo compiler lock.

**Goal:** Build the stripped, thin-LTO release binary (`D:\mystation\cargo-target\release\agents-station.exe`), certify 100% backend/OTLP/hook/security/soak parity via `scripts/run-oracle-contract-tests.mjs`, capture & visually inspect all release scene modes (`cozy`, `pro`, `hq`, `hud-pro`, `lite-2d`, `pro-country`, `pro-beach`, `pro-night`), verify every Windows Private Working Set budget (`serve/Tray ≤ 35–45 MB`, `2D Lite ≤ 110 MB`, `3D Pro + HUD ≤ 300 MB`, `3D HQ 44-desk ≤ 380 MB`, `0 powershell.exe`), and publish the final release certification report.

**Spec:** `docs/superpowers/specs/2026-11-25-agents-station-design.md`

## Hard Release Budgets & Acceptance Gates

| Mode | Hard Ceiling | Verification Method |
|---|---|---|
| `serve` daemon (70s soak) | **≤ 35 MB** Private WS, **0** `powershell.exe` | `scripts/run-oracle-contract-tests.mjs` against release binary |
| Tray Background (window hidden) | **≤ 45 MB** Private WS, **0** `powershell.exe` | Verified via `serve` / True Unload + `AdaptiveFpsState` 0 fps |
| 2D Lite / `/panel` | **≤ 110 MB** Private WS | Release `--capture-scene lite-2d` + Win32 `WorkingSetPrivate` |
| 3D Pro + City + Chibis + Full HUD | **≤ 300 MB** Private WS | Release `--capture-scene hud-pro` + Win32 `WorkingSetPrivate` |
| 3D HQ (44 desks) + City + Chibis | **≤ 380 MB** Private WS | Release `--capture-scene hq` + Win32 `WorkingSetPrivate` |
| Parity Ledger | **100% `[x]`** (0 `[ ]` entries) | `PARITY_LEDGER.md` audit (`461` files + `106` endpoints) |
| Workspace Tests | **100% PASS** | `cargo test --workspace` |

## Tasks

### Task 1: Build Optimized Release Binary (`agents-station.exe --release`)
- [ ] Run `cargo build --release --bin agents-station` under single compiler lock (`D:\mystation\cargo-target\release\agents-station.exe`).
- [ ] Record binary size and confirm `D:\mystation\cargo-target` disk footprint stays within budget.

### Task 2: Release Oracle Contract + 70s Soak + Zero-PowerShell Audit
- [ ] Run `STATION_BIN=D:/mystation/cargo-target/release/agents-station.exe node scripts/run-oracle-contract-tests.mjs`.
- [ ] Verify 100% endpoint/OTLP/hook/security pass, 70-second soak Peak Private WS ≤ 35 MB, and `0` `powershell.exe` child processes.

### Task 3: Release Visual Capture & Windows Private RAM Matrix
- [ ] Capture release PNGs (`artifacts/rel-{cozy,pro,hq,hud-pro,lite-2d,pro-country,pro-beach,pro-night}.png`) while sampling `Win32_PerfRawData_PerfProc_Process.WorkingSetPrivate` + child process tree.
- [ ] Inspect key release PNGs via `read_file` to verify zero visual regressions, zero tofu boxes, crisp Vietnamese/Thai text, and clean framing.
- [ ] If any mode exceeds its RAM budget (e.g. `2D Lite > 110 MB`), diagnose & fix immediately before closing Gate P6.

### Task 4: Final Certification Report, Full Test Pass & Push
- [ ] Write `docs/certification/2026-11-25-release-certification.md` with measured numbers, binary path, and test receipts.
- [ ] Confirm `git status` clean, `PARITY_LEDGER.md` has `0` unchecked `[ ]` items, `cargo test --workspace` green, and `git push origin main`.
