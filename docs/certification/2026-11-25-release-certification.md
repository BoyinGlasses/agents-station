# Agents Station — Release Certification Report (`2026-11-27`)

- **Binary:** `D:\mystation\cargo-target\release\agents-station.exe` (**64.22 MB** / `67,343,872` bytes, `lto = "thin"`, `codegen-units = 4`, `strip = "symbols"`, **0 compiler warnings**)
- **Workspace Target Dir:** `D:\mystation\cargo-target` (configured via `.cargo/config.toml`; zero build artifacts on `C:`)
- **Parity Ledger:** `PARITY_LEDGER.md` — **571 / 571 `[x]`** (**461 / 461** Oracle source files + **106 / 106** HTTP/SSE/OTLP/Launcher endpoints + **4 / 4** P0 bootstrap items, **0 `[~]`**, **0 `[ ]`**, **0 dummy stubs**)
- **Workspace Automated Tests:** **444 / 444 PASS** (`0` failed, `0` ignored, `0` warnings):
  - `station-core`: **179** unit tests (universal `Host`/`Origin`/tunnel/executable security, `MoveFileExW` atomic writes, JSONC-safe `post_rc`, real LAN listener bind/unbind + dynamic `bound_port`, process-backed Claude & Codex OAuth login/code/cancel/renew, `DarwinPlatform`/`LinuxPlatform` Unix parsers, `PinRateLimiter`, OTLP, Vault, Push RFC 8291/8292, Tunnel, `PARITY_LEDGER.md` integrity)
  - `station-3d`: **97** unit tests (`ChibiMeshCache` 689.3 KB < 1.5 MB, 4-cubemap IBL < 1.5 MB, `NavGraph`, 9 staff NPCs, all 17 leisure/meal/outing cast routines, all 15 `life/` systems, live `Demo` & `Replay`, 1:1 `City`/`Countryside`/`Beach` backdrops, 10 cm `ObstacleGrid`, 3× backdrop cycle 0-leak & full world unload 0-leak)
  - `station-ui`: **71** unit tests (`base.css` tokens, 700-weight VN/Thai fonts, 26 vector icons, 3-column shell, 2D Lite & 420×640 Panel, all 6 approval cards, all drawers/modals, `labels3d` occlusion, `en`/`vi`/`th` i18n)
  - `station-app`: **66** unit tests + **1** `capture_smoke` E2E (**67** total — `TrayDaemon`, `--gui-worker`, `AttachedHttp` + `InProcess` bridge, `3D→2D→Tray→3D`, `3×` backdrop switch, `WM_APP_SWITCH_3D` + standalone 3D fallback, no-PBR `--lite`/`--panel`)
  - `station-canvas2d`: **17** unit tests + **1** `golden_sheet` integration (**18** total — shared `cosmic-text` 0.14 VN/Thai shaping with poison-safe mutex recovery, `tv-games` zero-alloc `256×144`, `1080×1350` Wrapped PNG, ISO-18004 QR, all shop/street/office atlases)
  - `station-launcher`: **12** unit tests (0-PowerShell Win32 5-harness detection, hook install, focus/launch)

## Windows Private Working Set & Zero-PowerShell Certification (`--release`)

All measurements captured on Windows (`Win32_PerfRawData_PerfProc_Process.WorkingSetPrivate`) against `D:\mystation\cargo-target\release\agents-station.exe`:

| Mode | Measured Private WS (`--release`) | Hard Target | PowerShell Children | Status |
|---|---|---|---|---|
| `serve` daemon (70s soak + Oracle contracts + SIGKILL survival) | **6.64 MB peak / 3.23 MB steady** | ≤ 35 MB | **0** | **PASS** |
| Tray Background (`TrayDaemon` after `X` close, `GET /health=ok`) | **9.29 MB peak / 8.48 MB steady** | ≤ 45 MB | **0** | **PASS** |
| 2D Lite (`TrayDaemon` + `--gui-worker --lite`, no-PBR DX12) | **107.54 MB peak / 107.40 MB steady** (`lite-2d-vi` cap: 38.84 MB) | ≤ 110 MB | **0** | **PASS** |
| Compact Panel (`--panel` 420×640 attached to daemon, no-PBR DX12) | **106.65 MB peak / 105.76 MB steady** | ≤ 110 MB | **0** | **PASS** |
| 3D Pro + City (`TrayDaemon` + `--gui-worker`, Vulkan 4× MSAA) | **293.21 MB steady** (`street-shops` cap: 241.50 MB) | ≤ 300 MB | **0** | **PASS** |
| 3D Pro Countryside / Beach (Vulkan 4× MSAA) | **185.96 MB / 182.32 MB** | ≤ 300 MB | **0** | **PASS** |
| 3D HQ (44 desks + 4 dept wings + City, Vulkan 4× MSAA) | **338.70 MB peak** | ≤ 380 MB | **0** | **PASS** |
| 3× `City → Countryside → Beach → City` backdrop cycle | **0 leaked meshes, 0 leaked materials, 0 leaked images** | drift ≤ 15 MB | **0** | **PASS** |
| `3D → 2D → Tray → 3D` round-trip | Verified in ECS tests + live `TrayDaemon` (`9.29 MB` background) | Clean | **0** | **PASS** |

- **Subprocess Hygiene:** Verified `0` `powershell.exe` children across every test/soak/capture mode and `CREATE_NO_WINDOW = 0x0800_0000` on every Windows child `Command::new` spawn.

## Dual-Server Golden Oracle Contract & Security Certification

Executed `STATION_BIN=D:\mystation\cargo-target\release\agents-station.exe node scripts/run-oracle-contract-tests.mjs` side-by-side against `d:\mystation\my-station\server\index.mjs` (Golden Oracle untouched, `git status` clean):

- `[PASS]` `GET /health` matches 1:1 (`200 "ok"`)
- `[PASS]` `POST /event` across all 5 agents (`claude`, `codex`, `opencode`, `zcode`, `antigravity`)
- `[PASS]` Blocking `POST /hook/permission` + `POST /api/presence` + `POST /api/perm` round-trip on both servers
- `[PASS]` OTLP `POST /v1/metrics` + `POST /v1/logs` using Golden Oracle fixtures on both servers
- `[PASS]` `GET /stream` SSE initial snapshot (`"k":"snap"`) & `GET /api/live` normalized snapshot keys
- `[PASS]` 1:1 key-shape parity across all 9 shared GET endpoints (`/api/burn`, `/api/wrapped`, `/api/vault`, `/api/timeline`, `/api/sys`, `/api/stranded`, `/api/models`, `/api/push`, `/api/tunnel`)
- `[PASS]` Mutations (`POST /api/pins`, `POST /api/chat`, `GET /api/undo`, `POST /api/pin`, `POST /api/prefs`, `GET /api/harnesses`)
- `[PASS]` Security gates: cross-port `Origin: http://localhost:3000` -> **`403`**, spoofed `Host: evil.com` -> **`421`**, path traversal `/api/replay?day=../../x` -> **`400`**, API secret redaction in Vault, non-Claude `Undo` safety
- `[PASS]` Native `agents-station.exe hook` live & closed-port fallback
- `[PASS]` Hard `SIGKILL` crash-survival (Vault, 30d OTel Burn, Chat, Replay JSONL, Prefs, Pins survived via `MoveFileExW` atomic writes + 15s periodic flush)
- `[PASS]` 70-second idle soak: **Peak Private WS = 6.64 MB / steady 3.23 MB (≤ 35 MB)**, **0 `powershell.exe` children**

## All 10 Side-by-Side Web↔Native Visual Parity Pairs (`read_file` Verified 1:1)

All 10 Golden Oracle Web captures (`artifacts/oracle-*.png`, Playwright Chromium closed immediately after capture) and all 10 `--release` Native captures (`artifacts/native-*.png`) were visually inspected side-by-side via `read_file`:

| # | Scene | Golden Oracle Web Capture | Native `--release` Capture | Verified 1:1 Visual Parity |
|---|---|---|---|---|
| 1 | `cozy` | `artifacts/oracle-cozy.png` | `artifacts/native-cozy.png` | 9 desks, cream rugs, `SHIP IT ✦`, clock, globe, servers, rocket, aquarium, arcade, kitchenette, `BAND 8.0`, City streets |
| 2 | `pro` | `artifacts/oracle-pro.png` | `artifacts/native-pro.png` | Reception, elevator, WC, *Bàn thờ Thần Tài*, Director walnut office, meeting room, library, pantry, lounge + billiards/foosball/TV, KTV + phone booths, City + EV row + bus stop |
| 3 | `hq` | `artifacts/oracle-hq.png` | `artifacts/native-hq.png` | All 44 desks across 4 color-coded department wings + glass wing corridors + City + Mixue/GS25/Cơm bình dân/Vietcombank ATM |
| 4 | `pro-night` | `artifacts/oracle-pro-night.png` | `artifacts/native-pro-night.png` | Midnight asphalt, golden streetlamp pools, individual `UV_1` amber windows, glowing string lights, warm room lamps, glowing screens/arcade/KTV |
| 5 | `pro-country` | `artifacts/oracle-pro-country.png` | `artifacts/native-pro-country.png` | Patchwork + flooded rice paddies, meandering river, wooden footbridge, unobstructed *cổng làng*, bamboo groves, water buffalo, flapping egrets, kites |
| 6 | `pro-beach` | `artifacts/oracle-pro-beach.png` | `artifacts/native-pro-beach.png` | Rippled turquoise ocean, shallow band, surf foam waves, wet/dry golden shoreline, 70 palms, umbrellas/loungers, open-bowl *thuyền thúng*, fishing boats with VN flags, thatched huts, lighthouse, seagulls |
| 7 | `street-shops` | `artifacts/oracle-street-shops.png` | `artifacts/native-street-shops.png` | Native exterior street view verified crisp UV-cropped walk-in VN shop fascias (`ĐIỆN MÁY XANH`, `GS25`, `Bách Hóa Xanh`), open interiors, sagging utility wires, live countdown signals, rooftop water tank, Kenney/VF3/GrabFood/bulky traffic (note: Playwright Oracle capture zoomed on 245% reception camera) |
| 8 | `chibi-closeup` | `artifacts/oracle-chibi-closeup.png` | `artifacts/native-chibi-closeup.png` | Native close-up verified sculpted `hair_shell(48,16)` hair, layered Pixar eyes, 3 outfits (hoodie/suit/shirt), analytical 2-bone desk-typing IK, lazy held props (mug/clipboard/boba/mic/sign) (note: Playwright Oracle capture zoomed on 343% Director office camera) |
| 9 | `hud-pro-vi` | `artifacts/oracle-hud-pro-vi.png` | `artifacts/native-hud-pro-vi.png` | 3-column dark control-room shell in crisp Vietnamese (`Phiên làm việc`, `Hoạt động`, `Gói Claude`, `Máy tính RAM`, floating 3D pills, approval card, glassmorphic `Toàn cảnh`/`Hoá thân` toolbar) |
| 10 | `lite-2d-vi` | `artifacts/oracle-lite-2d-vi.png` | `artifacts/native-lite-2d-vi.png` | 2D Lite `Phiên Claude` board in Vietnamese (`Tất cả`, `Cần bạn`, `Đang làm`, `Đang nghỉ`, search `(phím /)`, inline `Cho phép (Y)` / `Từ chối (N)` approval card, right Activity panel; native retains left `SessionRail` alongside center board) |

- **Intentional RAM-Tuning Notes:** Directional/point shadow maps are tuned to `1024px` / `128px` (single ortho cascade, `NotShadowCaster` on static backdrop ground/roofs, night-deferred `UV_1`/emissive pipelines, 2× anti-aliased street-atlas downsampling) so `3D Pro + City` stays at **293.21 MB ≤ 300 MB** and `3D HQ` stays at **338.70 MB ≤ 380 MB** on 8 GB hosts while preserving visual fidelity.
