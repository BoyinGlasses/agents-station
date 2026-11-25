# Agents Station — Plan 5: `station-ui`, 2D Lite, Harness Launcher Dock, System Tray & Toast Approve

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task under a single Cargo compiler lock. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete native `bevy_egui` interface in `crates/station-ui` and native Windows lifecycle in `crates/station-app` covering all 240 P5 Oracle files: 105 `i18n/` (`en`/`vi`/`th`), 29 `styles/` design tokens + 25 TTF fonts, 42 `office/ui/` HUD/drawers/modals, 7 `panel/` + 20 `components/` + `app/` views, 2D Lite mode with true 3D unload (`≤ 110 MB`), **Native GUI Harness Launcher Dock** (`Claude`, `Codex`, `OpenCode`, `ZCode`, `Antigravity` + custom apps), Adaptive FPS (`60→10→0`), and native Windows System Tray + background server bridge (`≤ 45 MB` tray RAM).

**Architecture:** `station-ui` loads the synced `assets/i18n/{en,vi,th}.json` bundles at startup into `I18nStore`, applies the exact dark warm-slate + terracotta (`#D66440`) design token theme from `src/styles/*.css`, and renders the floating TopBar, SessionRail, Harness Launcher Dock, CameraToolbar, ActivityPanel, GroupChat, Permission/Denied/CI cards, and slide-over Drawers (`Session`, `System/Mac`, `Burn`, `Vault`, `Settings`, `Timeline`, `Wrapped`, `Phone QR`, `Whiteboard`). Switching to **2D Lite** despawns all `WorldEntity` objects via `station_3d::unload_office_world` and renders the crisp native 2D office board directly in `egui`.

**Tech Stack:** Bevy 0.16, `bevy_egui 0.34`, `station-core`, `station-launcher`, `station-3d`, Win32 `Shell_NotifyIconW` (zero extra heavy crates).

**Spec:** `docs/superpowers/specs/2026-11-25-agents-station-design.md`

## Global Constraints

- **Hard RAM Budgets:** 2D Lite ≤ 110 MB Private; 3D Pro + full HUD ≤ 300 MB; Tray background ≤ 45 MB; 0 `powershell.exe` spawns.
- **True Unload:** Switching 3D → 2D Lite or closing window to System Tray despawns 3D world entities and purges transient GPU images.
- **GUI Harness Launcher:** Clicking a harness button runs auto-hook preflight + focuses existing GUI window or launches native GUI app (`ShellExecuteW` / `shell:AppsFolder\<AUMID>`), NEVER spawning a terminal.
- **Full i18n:** `en`, `vi`, `th` switch dynamically at runtime with crisp `BeVietnamPro` + `PlexThaiLooped` + `JetBrainsMono` rendering.
- **Parity Ledger:** Mark all 240 P5 files `[x]` in `PARITY_LEDGER.md` upon Gate P5 completion.

## Review Focus

1. **Zero missing i18n keys & interpolation safety:** `I18nStore::tr("topbar.working", &[("n", "4")])` resolves nested dot keys across `en`, `vi`, `th` and safely falls back to `en` then raw key if missing.
2. **2D Lite True Unload & Visual Clarity:** Toggling `2D Lite` unloads 3D entities cleanly and renders live agent cards, permission approve/deny buttons, harness dock, and system gauges with `≤ 110 MB` RAM.
3. **Harness Dock interactive launch & custom app modal:** Lists all 5 built-in harnesses with real status badges (`Running`, `Ready`, `Needs Hook`, `Not Installed`), triggers `HarnessService::open()` on click, and lets users add/remove custom `.exe`/`.lnk` apps.
4. **Adaptive FPS & Live Server Sync:** `station-app` embeds the Axum HTTP/SSE server (`127.0.0.1:4310`) on a background Tokio thread so real hooks (`/event`, `/hook/permission`, `/v1/metrics`) drive the native `egui` HUD & 3D scene in real time.
5. **Visual Proof (`artifacts/g5-hud-pro.png` & `artifacts/g5-lite-2d.png`):** Capture full 3D Pro + native HUD and 2D Lite PNGs, visually inspect both via `read_file`, and verify RAM budgets.

---

### Task 1: `I18nStore` (`en`/`vi`/`th`) & Design-Token Theme (`theme.rs`)

**Files:**
- Modify: `crates/station-ui/Cargo.toml` (add `serde`, `serde_json`, `station-launcher`, `station-3d`)
- Create: `crates/station-ui/src/{i18n.rs,theme.rs}`
- Modify: `crates/station-ui/src/lib.rs`

- [ ] **Step 1: Implement `i18n.rs` loading `assets/i18n/{en,vi,th}.json` + dot-path lookup + `{var}` interpolation + unit tests**
- [ ] **Step 2: Implement `theme.rs` replicating all 29 CSS design tokens (`#14161E`, `#1E222E`, `#D66440`, `#3DDC84`, `#FFB23F`, `#FF5F5F`, `#F5F0E8`)**
- [ ] **Step 3: Run `cargo test -p station-ui` and commit**

---

### Task 2: Native HUD Shell — TopBar, SessionRail, Harness Launcher Dock & CameraToolbar

**Files:**
- Create: `crates/station-ui/src/{state.rs,hud.rs,launcher_dock.rs}`
- Modify: `crates/station-ui/src/lib.rs`

- [ ] **Step 1: Implement `UiState` resource (active drawer, language, layout, backdrop, 2D Lite toggle, night mode, streamer mode, GPU preference, adaptive FPS target, live sessions/perms/harnesses/sys snapshot)**
- [ ] **Step 2: Implement `hud.rs` (TopBar with plan/clock/weather/status counts/mode switches, SessionRail left cards, CameraToolbar bottom bar, Permission/Denied/CI approval banners, Toast stack) and `launcher_dock.rs` (5 built-in GUI harnesses + custom app editor)**
- [ ] **Step 3: Run `cargo test -p station-ui` and commit**

---

### Task 3: All Drawers, Modals, Group Chat, Panel Mode & 2D Lite View

**Files:**
- Create: `crates/station-ui/src/{drawers.rs,lite_view.rs}`
- Modify: `crates/station-ui/src/lib.rs`

- [ ] **Step 1: Implement `drawers.rs` covering `SessionDrawer` (compose/pins/undo/account/models/tasks), `SystemDrawer` (RAM/CPU/disk/ports/dev-servers), `BurnDrawer` (OTLP cost/token breakdown), `VaultDrawer` (search + prompt coach report), `GroupChatDrawer`, `SettingsPanel` (language/layout/backdrop/chibi look/GPU/weather/sound/auto-close), `TimelineDrawer`, `WrappedModal`, `PhoneQrModal`, `WhiteboardDrawer`**
- [ ] **Step 2: Implement `lite_view.rs` covering 2D Lite full-window office grid + `/panel` compact card view (runs with zero 3D entities!)**
- [ ] **Step 3: Run `cargo test -p station-ui` and commit**

---

### Task 4: Adaptive FPS, Live Embedded HTTP Server Bridge & Native Windows System Tray / Toast (`station-app`)

**Files:**
- Create: `crates/station-app/src/{tray.rs,fps.rs}`
- Modify: `crates/station-app/src/main.rs`, `crates/station-ui/src/lib.rs`

- [ ] **Step 1: Implement `fps.rs` (60 fps focused → 10 fps unfocused → 0 fps minimized/tray) and `tray.rs` (zero-PowerShell Win32 tray status & notification helper + true 3D unload on close-to-tray)**
- [ ] **Step 2: Wire `StationUiPlugin` + live `AppState` bridge into `station-app/src/main.rs` and support `--capture-scene hud-pro` and `--capture-scene lite-2d`**
- [ ] **Step 3: Build, test, and commit**

---

### Task 5: Visual & RAM Certification Gate P5 (`g5-hud-pro.png`, `g5-lite-2d.png`)

- [ ] **Step 1: Capture `artifacts/g5-hud-pro.png` and `artifacts/g5-lite-2d.png`, visually inspect both via `read_file`**
- [ ] **Step 2: Measure Windows Private Working Set for 2D Lite (`≤ 110 MB`) and 3D Pro + full HUD (`≤ 300 MB`)**
- [ ] **Step 3: Mark all 240 P5 files `[x]` in `PARITY_LEDGER.md`, run `cargo test --workspace`, commit, and push Gate P5 to `origin/main`**
