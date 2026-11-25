# Agents Station — Audit Remediation & 1:1 Visual/UI Parity Specification

**Date:** 2026-11-25
**Status:** Approved
**Target Repository:** `d:\mystation\agents-station`
**Golden Oracle (Read-Only):** `d:\mystation\my-station`
**Audit Baseline:** Deep review of commit `c76d7ea` (`PARITY_LEDGER.md` falsely marked `571/571 = 100%` at ~25–30% real parity).

---

## 1. Execution Sequence & Anti-Shortcut Mandate

Execution is strictly ordered into **two stages across 5 sequential subsystem plans**:
1. **Stage 1 — Fix All Audit Bugs, Security Holes, Lifecycle Crashes & Backend/Live Stubs (`Plan R1` → `Plan R2`).**
2. **Stage 2 — Overhaul Entire UI Shell, i18n, Render Pipeline & 3D World for True 1:1 Parity with Web (`Plan V1` → `Plan V2` → `Plan V3`).**

**Hard Anti-Cheat Rules:**
- Reset every stubbed/simplified row in `PARITY_LEDGER.md` to `[ ]` in `Plan R1` Task 1 before fixing anything. A row may only return to `[x]` when backed by a real test AND (for `V1–V3`) a side-by-side PNG comparison against `d:\mystation\my-station`.
- Zero dummy handlers (`post_local_ok`, `get_generic_ok`, `post_accounts_any`, `=> {}`) may remain at completion of `Plan R2`.
- Single Compiler Lock on 8 GB host (`target-dir = "D:/mystation/cargo-target"`, `jobs = 2`; never run two concurrent `cargo` processes).
- Runtime Private Working Set ceilings remain mandatory: `serve` `≤ 35 MB`, Tray `≤ 45 MB`, 2D Lite `≤ 110 MB`, 3D Pro `≤ 300 MB`, 3D HQ `≤ 380 MB`, `0` `powershell.exe` spawns.

---

## 2. Stage 1 Specification — Security, Lifecycle, Persistence & Full Live Backend (`R1` & `R2`)

### 2.1 Security & Input Boundaries (`R1`)
- **Strict Origin & Host Guard (`crates/station-core/src/http/guard.rs`):**
  - Reject **any** request carrying an `Origin` header on `/event`, `/hook/*`, and `/v1/*` (`403 Forbidden`).
  - Validate `Host` on **every** HTTP route (`127.0.0.1[:port]`, `localhost[:port]`, `[::1][:port]`, or configured tunnel domain; return `421 Misdirected Request` otherwise — blocks DNS rebinding).
  - On local browser mutation routes (`/api/*`), if `Origin` is present, require exact `scheme://host:port` match against `Host` (cross-port `http://localhost:3000` → `:4310` MUST fail with `403`).
  - Remove client-spoofable `x-office-via-tunnel` header trust; tunnel requests arrive only via a dedicated loopback tunnel listener socket tagged server-side (`ConnectInfo` / router extension `ViaTunnel(true)`).
- **Custom Harness Target Validation (`crates/station-launcher`):**
  - `mutate_custom` MUST reject URIs, UNC paths (`\\...`), shell commands, and non-existent files; accept only existing local `.exe` or `.lnk` paths (or verified MSIX AUMIDs) and auto-populate `process_names` from the executable stem/filename when omitted.
- **Replay Traversal, PIN Lockout, Phone XSS & Secret Redaction (`R1`):**
  - `GET /api/replay` MUST validate `day` against `^\d{4}-\d{2}-\d{2}$` (`400` otherwise).
  - `POST /pin` & `POST /api/pin` MUST enforce 5 failed attempts / 15 min per IP and 20 failed attempts global lockout (`429 Too Many Requests`).
  - `POST /api/perm` from non-loopback MUST require `prefs.get("phoneApprove") == true` and reject items where `danger.is_some()`.
  - Escape all dynamic strings in `/phone` HTML (`textContent` / `escape_html`).
  - Apply `redact()` to `post_chat` text before storing in `chat.json` and recursively redact string leaves of `body` in `post_hook_denied` before `chat.deny_card()`.
- **Win32 Safety & Non-Claude Undo (`R1`):**
  - `platform/win32.rs`: allocate `Vec<u32>` (4-byte aligned) for `GetExtendedTcpTable` buffers; check `GetExitCodeProcess(h, &mut code) != 0 && code == 259` (`STILL_ACTIVE`) in `is_pid_alive`; guard `system.rs::descendants` with `HashSet<u32>`.
  - `state/undo.rs`: allow `:` in `valid_sid` (`^[A-Za-z0-9_:-]{1,120}$`), map `:` → `_` for filesystem directory names, check `symlink_metadata` (skip symlinks), require target path canonicalizes inside `session.cwd`, and cap stored sessions to `50` (evicting oldest disk folders).

### 2.2 Crash-Safe Persistence, Reaper & Service Coexistence (`R1`)
- `atomic.rs`: on Windows, replace `remove_file + rename` with `MoveFileExW(tmp, dst, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`.
- Save `vault`, `trophies`, and `tenure` immediately on mutation (debounced ≤ 2s for high-rate events); run a 15-second background task flushing dirty `OtelStore` and reaping dead/idle sessions (`autoCloseHours`), expired `pending_perms`, and stale `undo` dirs; replace mutex `.expect(...)` with `.unwrap_or_else(|e| e.into_inner())`.
- Append every normalized live event to `data/replay/YYYY-MM-DD.jsonl` (local date) so `/api/replay`, `/api/timeline`, and `/api/wrapped` (fixed to `Local` timezone + `"30d"` burn support) read real history.
- Anchor `resolve_runtime_data_dir()` to the executable/repo root (when `OFFICE_DATA_DIR` is unset), never arbitrary `current_dir()`.
- If `127.0.0.1:4310` is already serving `/health == "ok"` when GUI starts, attach `LiveServerBridge` as an HTTP + `/stream` SSE client to that daemon instead of silently creating a split-brain `AppState`; surface any genuine bind error as a visible banner.

### 2.3 Persistent System Tray, `< 45 MB` Unload & `3D ↔ 2D Lite` Lifecycle (`R1`)
- Separate architectural architecture cleanly:
  - Keep a persistent Win32 message-only window (`TrayHost`) with real embedded app icon (`CreateIconIndirect`), `NIF_ICON | NIF_MESSAGE | NIF_TIP`, left-click restore, right-click menu (`Open 3D Office`, `Open 2D Lite`, `Quit`), and named Win32 event wakeup when a second `agents-station.exe` starts.
  - Intercept `WindowCloseRequested`: when `close_to_tray` is enabled (default `true`), hide/close the renderer surface rather than exiting `:4310`.
  - To guarantee **`≤ 45 MB` Private WS** in Tray mode (without keeping Bevy's ~90 MB Vulkan/DX12 swapchain & `wgpu::Device` resident): run the `:4310` daemon + `TrayHost` in the lightweight parent process (`~8–20 MB`) and spawn `agents-station.exe --gui-worker` for the Bevy window (terminating `--gui-worker` on Minimize-to-Tray and respawning on Tray Open in `< 0.5s`!), **AND** within `--gui-worker` support seamless `3D ↔ 2D Lite` switching by keeping `LiteCamera` separate from `WorldEntity`, registering `PbrPlugin` always, and calling `rebuild_office_world(...)` on `ViewMode::Mode3D` / `SwitchLayout`!
  - Guard `camera_control_system` with `egui` `wants_keyboard_input()` / `wants_pointer_input()`, and drop to `0 FPS` when `Window::is_minimized()`.

### 2.4 Blocking Hooks, Full Backend Wiring, Live 3D Bridge & Audio (`R2`)
- **Blocking Hooks & Toast Approval:** Hold `/hook/permission`, `/hook/question`, and `/hook/denied` on `tokio::sync::oneshot` channels governed by `mac_hold::hold_plan` + `X-Office-Wait`; resolve from UI cards, `POST /api/perm`, `POST /api/denied`, `POST /api/panel/answer`, and interactive WinRT/Win32 Toast notifications when unfocused/in-tray.
- **Zero Stub Routes & Zero Dead Modules:** Wire all 19 dead `station-core` modules (`auto_resume`, `bg_sessions`, `git`, `mac_hold`, `steer`, `stranded`, `testrun`, `agent_history`, `agent_rpc`, `antigravity_runs`, `cli_command`, `codex_history`, `shots`, `usage`, `vsix`, `comnha`, `duty`, `model_name`, `quick_choices`) into `AppState` and replace all 31+ stubbed routes (`tasks/*`, `stranded/*`, `timeline`, `image`, `prompt/rewrite`, `models` writing `~/.claude/settings.json` + `CLAUDE.md`, `claude statusLine` hook, `~/.claude/projects` gzip/accent-folded `vault`, real `codex`/`antigravity`/`claude` account managers, real P-256 RFC 8291/8292 `Web Push`, `cloudflared` tunnel, and interactive `/phone` PWA).
- **Live 3D & Audio Bridge:** Remove `with_demo_seed` from live mode; sync `OfficeState` unconditionally on `seq` change (aligning `"work"`/`"wait"`/`"rest"`) to both `UiState` and 3D `ChibiWorker`s, `DeskSlot` screens, whiteboard, TV, duty board, dev-server boxes, and `bevy_picking` observers; play all 27 procedural WAVs via Win32 `PlaySoundW(..., SND_MEMORY | SND_ASYNC)` (`winmm`, 0 heavy audio deps); move launcher process/window polling to a 1 Hz background worker using `IsIconic(hwnd)` before `SW_RESTORE`.

---

## 3. Stage 2 Specification — 1:1 Web UI & 3D Visual Parity (`V1`, `V2`, `V3`)

### 3.1 Exact Control-Room UI Shell & CSS Tokens (`Plan V1` — matching `src/styles/base.css`, `topbar.css`, `sidebars.css`, `stage.css`)
- **Exact Palette:** `--bg: #0D1219`, `--bar: #111720`, `--side: #131922`, `--card: #18202B`, `--card-2: #1E2734`, `--card-3: #263040`, `--line: rgba(255,255,255,0.07)`, `--line-2: rgba(255,255,255,0.13)`, `--ink: #E8ECF4`, `--muted: #8D97AA`, `--faint: #5E687B`, `--green: #3DDC84`, `--amber: #F2B544`, `--accent: #EF8A5F`, `--accent-hi: #F5A07A`, `--red: #FF5F5F`, `--blue: #6AAEFF`.
- **Exact 3-Column × 2-Row Shell (`≥ 1100px`):**
  1. **`TopBar` (`56px` height, `#111720`):** Brand logo (`#F08A3E`) + `"Agents Station"` + `HH:MM` clock + `DEMO`/`REPLAY` chip + `3D | 2D` segmented switch on left; **Apps (Harnesses) pill/popover** + `Plan` quota pill (`84×6px` bar + `Claude | Codex | Antigravity` popover) + `PC/RAM` pill (`84×6px` bar) + `Chat` (with unread badge) + `Replay` + `Settings` (`⚙` popover) + `Phone` icon + `Night` moon icon on right.
  2. **`SessionRail` (`330px` left panel, `#131922`):** Title + live green dot + active summary + 3 filter tiles (`work` / `wait` / `rest`) + search input (`🔍`) + scrollable session list with nested dashed subagent rows (`ri-subs`). Collapses to top stage `.chips` at `< 1100px`.
  3. **`ActivityPanel` (`350px` right panel, `#131922`):** Header + `live` indicator + 5 KPI tiles (`Tasks`, `Read`, `Edit`, `Cmd`, `$ API`) + `Timeline` card + `Weekly Wrapped` card + `[All | Attention]` filter + `[Cmd | Edit | Read | Web | Agent]` type chips + live event stream.
  4. **Center `Stage` (`1fr`, `Camera.viewport` cropped strictly to central rect):**
     - In `3D` mode: radial sky backdrop gradient (`#DCE6EE → #93A7B8` day / `#2A3555 → #0E1424` night), 3D projected floating character tags / bubbles / zone chips (`world_to_viewport` with depth sorting & declutter), floating `PlaceCard`, `perm-stack` (Permission, Question, Plan, Denied, Stuck, CI cards), `FirstPersonHud` (crosshair, help bar, `E` near prompt, `bil-hud`), `ReplayBar`, `PhotoBar`, and bottom-center glass `CameraToolbar` (`[Home | Desks | Meeting | Server]` hover-expanding buttons + `[- 100% +]` + `[Fullscreen | Photo | List]` + coral `[🚶 Walk in]`).
     - In `2D` mode: full Oracle `lite.js` session board inside center column (grouped `Working` / `Waiting` / `Resting` cards with git branch/dirty, cache warmth, context `%`, todos, subagents, keyboard navigation).
- **100% `I18nStore` Coverage:** Every visible UI string resolved via `i18n.tr` / `tr_vars` (`en`, `vi`, `th`), supporting plural objects (`one`/`other`), random phrase arrays, and `{mac}`/`{Mac}` substitution.
- **Complete Slide-Over Drawers & Modals:** Full ports of `SessionDrawer` (markdown transcript, Steps/Subagents/Activity tabs, Compose + Prompt Coach + Rewrite + Steer + Undo + Pins + Account picker), `GroupChat` (multi-agent threads, reactions, replies, recall, polls, reminders, stickers, photos), `Whiteboard` (interactive sticky notes synced to `/api/todos` & 3D wall texture), `Photo` (Screen/FHD/4K PNG + nametags) & 15s `Timelapse` (GIF/MP4 + lo-fi synth), `Wrapped` (1080×1350 PNG export), `Burn` (`Today | 7d` stacked bars + MCP/skill/role/project rankers), `Vault` (transcript viewer, `--resume`, weekly Prompt Coach report), `MacDrawer` (process table, 2-click cleanups, stranded git repos), `PhoneModal` (QR image, PIN edit, LAN, Tunnel, Push), and standalone `/panel` mode.

### 3.2 Exact 3D Render Pipeline, Rooms & Props (`Plan V2`)
- **Render Pipeline (`renderer.js` + `lights.js` parity):**
  - `Msaa::Sample4` on desktop (`Off` in `--lite`), `Tonemapping::AcesFitted`, `Exposure { ev100: 9.54 }` (~1.12× linear exposure match), night/emissive `Bloom` (`intensity: 0.18`), procedural Day Studio & Night Warm/Cool cubemap `EnvironmentMapLight` (generated once in memory, `< 1.5 MB` GPU), dual-hemisphere lighting (sky `#FFF4DC` + upward bounce directional `#A9B98A` so vertical & underside faces separate cleanly!), `DirectionalLightShadowMap { size: 2048 }` with tight layout-matched ortho `CascadeShadowConfig` (`±17m` Cozy, `±21m` Pro, `±28m` HQ), and dynamic `sync_lighting_system` for day/night/occupancy room lamps.
- **Text & Procedural Surfaces:** Replace `ab_glyph` with shared `cosmic-text` (`FontSystem` + `SwashCache`, covering Vietnamese, Thai GSUB/GPOS, `≤`, and Hangul fallback); add procedural `Canvas2d` generators for `plankTex` wood floors, pantry check/subway tiles, server raised-floor grid, speckle rugs, wainscoting, day/night `windows.js` paintings + roller blinds, room door plaques, and HQ dynamic department signs & Kanban sprint boards; fix `BackdropAssets` material leak.
- **100% Room & Prop Geometry (`pro/floor.js`, `pro/wing.js`, `furniture/*`, `props/*`):**
  - Pro floor: cut exact doorway gaps + glass double doors, walnut U-shaped reception counter + `My Office` sign + waiting sofa, 2 restroom stalls with occupancy indicators + sink/mirror + `WC` sign, 4 pantry dining tables + 16 colored chairs + 2 water coolers + sink faucet, 2 acoustic phone booths, `SHIP IT` wall sign, ticking 3-hand analog wall clock, framed wall art, floor lamps, spinning globe, library armchairs + 7-rung wooden ladder, lounge beanbag + foosball table, server AC cooler with live CPU/°C screen + blinking rack fans/LEDs, rocket launch + smoke + 70-piece confetti.
  - All 22 props: **3 animated/playable pixel-art `tv-games`** (`football`, `racing`, `fighting` + ambilight), live `arcade`, 16-ball pocketed `billiards` + cue rack, furnished `karaoke` booth + scrolling lyrics, 2 hollow `microwave` ovens + 5 lunchboxes + steam, `shredder` + per-desk context paper stacks & waste `bins`, per-desk terracotta cache-warmth `cups` + steam, 20-part articulated wandering `cat` (`meow` + hearts), gravel/seaweed/bubble `aquarium` (fish facing velocity + click feed), nav-graph `vacuum` (parented LED!), per-owner `dev-servers`, `printer` paper eject, `switches`, and 7 desk `trinkets` + laptop sticky notes + coats/backpacks + 4-stage git-dirty desk mess + tilted laptop screen fix.

### 3.3 Exact Sculpted `ChibiRig`, People, Life & Vietnamese Backdrops (`Plan V3`)
- **Sculpted Indexed `ChibiRig` (`< 2 MB` total shared VRAM):**
  - Port exact mathematical generators from `chibi-rig.js`: `hair_shell(NI=48, NJ=16)` (indexed smooth normals — 11× smaller than JS while visually identical!) + Catmull-Rom `sweep` for all 6 hairstyles (`bob`, `plain`, `spiky`, `ponytail`, `bun`, `curly`), Lathe `torso` profile (`Toy` 2-head & `Pixar` 2.6-head), layered eyes (sclera, colored iris, pupil, 2 white highlights, upper lash, brow, blinking lids), ears/nose/mouth, 3 outfits (`hoodie`, `suit`, `shirt`), hip→knee→shoe & shoulder→elbow→hand hierarchy with 2-bone analytic arm IK (`apply_chibi_pose`), soft radial floor shadow quad, outerwear/hats/lanyards, and all 15+ held props spawned lazily only while active.
- **Navigation, Subagents, 26 NPC Modules & 15 `life/` Systems:**
  - Port `NODES`/`ADJ` room-door walk graph, live worker walk-in/station-walk/leave, `0.78x` subagent chibis + dashed `team-lines`, and the full cast & cultural routines (`director` + `rounds`, `hr` clock-in/escort/birthday, `accountant`, `staff` receptionist/cleaner/Shopee, `guard`, `interns`, `farewell`, 2-player `billiards`/`foosball`/`gaming`, tone-synth `karaoke`, noon `comnha`/`comtrua`, `breaks`/`teatime`, street `outings`, evening `nhau`, `payday` ATM, release `photobooth`, `pickle`, `evswap`, `duty` chores, `errands`, `gatherings`) + all 15 `life/` modules (detailed red-gold *Bàn thờ Thần Tài – Thổ Địa*, *Diễn tập PCCC* fire drill, lift maintenance, Halloween/Christmas/Tết `seasons`, Open-Meteo `weather` + `rain` drops & wet-floor cone, wall `post` box, `incidents`, `quota`, server `tests` board, persisted `trophies` & `photostrips`).
- **1:1 Exterior Backdrops (`City`, `Countryside`, `Beach`):**
  - `City`: multi-block street grid, sidewalks, streetlamps with night pools, utility poles & wires (`street-wires`), animated countdown signals (`street-signals`), 13 `cars.json` vehicles + VinFast/Grab/XanhSM/buses + zebra pedestrians (`street-life`, `street-folk`, `street-riders`, `street-shade`, `street-siren`), **all 10 walk-in 3D Vietnamese shops** (`Cộng`, `Highlands`, `Phúc Long`, `Mixue` + Snow King, `TGDĐ` + tube dancer, `ĐMX`, `Katinat`, `Long Châu`, `GS25`, `Circle K`, `WinMart+`/`BHX`), *Quán cơm bình dân / Bún đậu* (`eat-com`), traffic police checkpoint (`street-csgt`), morning vendors (`street-morning`), *xe ôm* / hammock / barber / tire repair (`street-corner`, `street-repair`), lunar votive bins (`street-lunar`), *công viên dưỡng sinh / đá cầu / cờ tướng* (`street-park`), evening *quán nhậu* (`street-nhau`), weekend *rạp cưới* (`street-wedding`), and properly UV-mapped 3D `street-atm`, `street-ev` (scooter row + battery bars), `street-photo`, and `street-pickle` court.
  - `Countryside` (meandering river, wooden bridge, *cổng làng*, bamboo/banana, water buffalo, egrets, kite kids, 260 night fireflies) & `Beach` (vertex-colored shoreline, rolling surf foam, 70 palms, loungers, *thuyền thúng*, bobbing boats, thatched huts, rotating night lighthouse beam, seagulls) — with full asset purge on backdrop switch (`≤ 15 MB` drift across 3 cycles).
