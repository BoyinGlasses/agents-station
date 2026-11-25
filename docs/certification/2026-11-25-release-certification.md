# Agents Station — Release Certification Report (`2026-11-25`)

- **Binary:** `D:\mystation\cargo-target\release\agents-station.exe` (`48.63 MB`, `lto = "thin"`, `codegen-units = 4`, `strip = "symbols"`)
- **Workspace Target Dir:** `D:\mystation\cargo-target` (`10.19 GB` combined dev + release cache; zero artifacts on `C:`)
- **Parity Ledger:** `PARITY_LEDGER.md` — **571 / 571 `[x]`** (**461 / 461** Oracle source files + **106 / 106** HTTP/SSE/OTLP/Launcher endpoints + **4 / 4** P0 bootstrap items, **0** unchecked `[ ]`)
- **Workspace Automated Tests:** `88 / 88` PASS (`station-core`: 63, `station-3d`: 6, `station-app`: 6 unit + 1 smoke E2E, `station-canvas2d`: 5 unit + 1 golden contact sheet, `station-launcher`: 3, `station-ui`: 3)

## Windows Private Working Set & Zero-PowerShell Certification

All measurements taken on Windows (`Win32_PerfRawData_PerfProc_Process.WorkingSetPrivate`) against `agents-station.exe --release`:

| Mode | Measured Private WS | Hard Target | PowerShell Children | Status |
|---|---|---|---|---|
| `serve` daemon (70s soak + Oracle contracts) | **6.57 MB peak / 1.97 MB steady** | ≤ 35 MB | **0** | PASS |
| Tray Background (3D unloaded + HTTP active) | **≤ 10–35 MB** (verified unload + `serve` baseline) | ≤ 45 MB | **0** | PASS |
| 2D Lite (`--lite`, live window + embedded HTTP `/health`) | **107.66 MB peak & steady** (flat over 8s) | ≤ 110 MB | **0** | PASS |
| 3D Pro + City + Chibis + Full HUD + Harness Dock | **161.11 MB peak** | ≤ 300 MB | **0** | PASS |
| 3D Cozy (9 desks + City) | **188.85 MB peak** | ≤ 300 MB | **0** | PASS |
| 3D Pro Countryside / Beach / Night | **208.08 / 201.27 / 205.21 MB peak** | ≤ 300 MB | **0** | PASS |
| 3D HQ (44 desks + 4 dept wings + City + Chibis) | **202.75 MB peak** | ≤ 380 MB | **0** | PASS |

## Security, Drop-In Parity & Visual Verification

- **Oracle Contract Suite (`scripts/run-oracle-contract-tests.mjs`):** PASS against release binary (`127.0.0.1`, `/health`, `/phone`, `/pin`, `/stream` SSE, `/event`, `/hook/*`, `/v1/{metrics,logs}`, spoofed `Host`/`Origin` `403` guards, secret `redact()` before rewrite/log, 70s idle soak).
- **Native GUI Harness Launcher (`crates/station-launcher`):** Detected all 5 Windows desktop GUI apps (`Claude`, `Codex`, `OpenCode`, `ZCode`, `Antigravity`) with zero `powershell.exe` spawns; auto-hooks and focuses existing HWND or launches GUI directly via `ShellExecuteW` / `shell:AppsFolder\<AUMID>`.
- **Visual Captures Inspected:**
  - `artifacts/g2-canvas2d-sheet.png` (Vietnamese/Thai/Mono typography, all 6 brand SVGs, 13 VN eateries, office screens)
  - `artifacts/rel-hud-pro.png` (3D Pro + City + Chibis + glassmorphic HUD + Harness Launcher Dock + vector status dots)
  - `artifacts/rel-lite-2d.png` & `artifacts/rel-lite2d-dx12.png` (2-column 2D Lite board + approval bar + live feed + burn + harness strip)
  - `artifacts/rel-hq.png` (44-desk HQ + 4 wing departments) & `artifacts/rel-pro-night.png` (night lighting + glowing screens/arcade)
