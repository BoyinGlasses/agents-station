# Agents Station — Plan V1: 1:1 Web Control-Room UI Shell, Full `I18nStore`, 3D Floating Labels & Complete Drawers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild `station-ui` 1:1 to match `my-station`'s exact dark control-room shell (`56px` `TopBar` + `330px` `SessionRail` + `1fr` `Stage` with cropped `Camera.viewport` & floating `CameraToolbar` + `350px` `ActivityPanel`), wire `I18nStore` (`en`/`vi`/`th`) across 100% of visible UI strings, add 3D-projected floating character/zone labels (`CSS2DRenderer` parity), and replace every stub drawer/modal/2D-Lite/`/panel` view with full Golden Oracle parity.

**Architecture:** `egui::TopBottomPanel::top` (`56px`), `egui::SidePanel::left` (`330px`), and `egui::SidePanel::right` (`350px`) frame the central stage (`base.css:28`), while a Bevy system dynamically crops `MainOfficeCamera`'s `Camera.viewport` to `ctx.available_rect()` (collapsing sidebars below `1100px`/`720px` or in fullscreen/photo mode). Floating stage elements (`CameraToolbar`, 3D→2D projected tags/bubbles/chips, `PlaceCard`, `perm-stack`, `FirstPersonHud`, `ReplayBar`, `PhotoBar`) render in an overlay pass bounded to the central stage rect, and right-hand slide-over drawers (`Session`, `Mac/System`, `Burn`, `Vault`, `GroupChat`, `AI Accounts`) match `drawers.css` & `modal.css` 1:1.

**Tech Stack:** Rust 1.97+, `bevy` `0.16`, `bevy_egui` `0.34`, `pulldown-cmark` (lightweight markdown rendering in Session/Chat drawers), `qrcode` (or existing `station_canvas2d::draw_qr_block` rasterizer for Phone QR).

**Spec:** `d:\mystation\agents-station\docs\superpowers\specs\2026-11-25-remediation-and-1to1-visual-spec.md`

## Global Constraints

- Prerequisite: Stage 1 (`Plan R1` + `Plan R2`) completed and green.
- Single Compiler Lock (`target-dir = "D:/mystation/cargo-target"`, `jobs = 2`).
- Exact CSS palette from `d:\mystation\my-station\src\styles\base.css`: `--bg #0D1219`, `--bar #111720`, `--side #131922`, `--card #18202B`, `--card-2 #1E2734`, `--card-3 #263040`, `--ink #E8ECF4`, `--muted #8D97AA`, `--faint #5E687B`, `--green #3DDC84`, `--amber #F2B544`, `--accent #EF8A5F`, `--accent-hi #F5A07A`, `--red #FF5F5F`, `--blue #6AAEFF`, logo `#F08A3E`.
- ZERO hardcoded user-facing English labels in `station-ui` (`grep` gated in Task 8) — every label MUST resolve through `I18nStore` (`en`, `vi`, `th`).
- Register `FontBundle.ui_bold` (`BeVietnamPro-700`) as a named `egui` font family so headings/badges render true 700-weight Vietnamese/Thai typography.
- Every visual task MUST export `--capture-scene` PNGs (`en` and `vi`) and visually verify side-by-side against `d:\mystation\my-station` via `read_file`.

## Review Focus

1. **`I18nStore` returning raw keys or empty strings on plural objects (`{"one","other"}`), phrase arrays, or `{mac}`/`{Mac}` placeholders** — `tr_vars("time.min", &[("n", "5")])` and `{Mac}` → `"PC"` must resolve cleanly across `en`, `vi`, and `th` with zero raw key leaks; pinned by unit test in Task 1.
2. **3D office off-center or stretched underneath `330px` left and `350px` right sidebars** — `Camera.viewport` physical position & size (scaled by `window.scale_factor()`) must track `CentralPanel` bounds every frame (clamped safely when window minimizes to `0×0` so `wgpu` never panics on zero-sized viewport!); pinned by test in Task 2.
3. **3D floating name tags / bubbles rendering behind camera (`ndc.z < 0`) or overlapping into unreadable stacks** — projection system must cull behind-camera points (`world_to_viewport` returning `None`) and run priority rectangle decluttering matching `place-chips.js`; pinned by unit test in Task 3.
4. **Clicking `2D` in TopBar hiding sidebars instead of replacing only the center stage, or keyboard shortcuts firing while typing in search** — `2D` mode (`lite.js`) must render inside the center column between `SessionRail` and `ActivityPanel` (matching `my-station-latest-check.png`!), while standalone `/panel` (`--panel` / compact toggle) renders the compact window; pinned in Task 4.
5. **Interactive cards (`AskUserQuestion` multi-choice, `ExitPlanMode` Proceed/Feedback, `PermissionDenied` Retry, `Stuck` loop stop/hint, `CI` fix) missing from `perm-stack` / `GroupChat`** — all 6 card types must render interactive controls wired to `station-core`; pinned in Tasks 5 & 6.

---

## File Structure

- Modify: `crates/station-ui/src/{theme.rs,fonts.rs,i18n.rs,lib.rs,state.rs}` — exact `base.css` tokens, registered `bold` font family, full plural/array/`{Mac}` `I18nStore`.
- Create: `crates/station-ui/src/icons.rs` — crisp 16×16 vector painter icons matching `src/office/ui/icons.js` (`logo`, `home`, `monitor`, `users`, `server`, `walk`, `camera`, `list`, `maximize`, `chat`, `rewind`, `gear`, `phone`, `moon`, `search`, `terminal`, `pencil`, `file`, `globe`, `trophy`, `timeline`).
- Create: `crates/station-ui/src/shell/{mod.rs,topbar.rs,session_rail.rs,activity_panel.rs,camera_toolbar.rs,stage_overlays.rs}` — replaces old `hud.rs` & `launcher_dock.rs` with exact `TopBar.tsx`, `SessionRail.tsx`, `ActivityPanel.tsx`, `CameraToolbar.tsx` + native Apps/Harnesses TopBar popover & rail quick-launch, `FirstPersonHud`, `ReplayBar`, `PhotoBar`, `PlaceCard`, and `perm-stack`.
- Create: `crates/station-ui/src/labels3d.rs` — 3D→2D projected worker name tags, tool badges, speech/thought bubbles, and zone chips with priority decluttering.
- Modify: `crates/station-ui/src/lite_view.rs` + Create: `crates/station-ui/src/panel_view.rs` — full `lite.js` center board + standalone `/panel` view (`src/panel/*.js`).
- Create: `crates/station-ui/src/drawers/{mod.rs,session.rs,group_chat.rs,mac_sys.rs,burn.rs,vault.rs,accounts.rs,whiteboard.rs,modals.rs}` — replaces stub `drawers.rs` with full-featured slide-over drawers & modals (`PhoneQr`, `Wrapped` 1080×1350 PNG export, `Photo`/`Timelapse`, `AddHarness`).

---

### Task 1: Exact `base.css` Design Tokens, `700` Bold Font Registration, Vector Icons & Complete `I18nStore`

**Files:**
- Modify: `crates/station-ui/src/{theme.rs,fonts.rs,i18n.rs,lib.rs}`
- Create: `crates/station-ui/src/icons.rs`

**Interfaces:**
- Produces:
  - Exact color constants in `theme.rs` matching `base.css:6-15` + `heading_bold(text, size)` helper using `FontFamily::Name("bold".into())`.
  - `I18nStore::tr(&self, key: &str) -> String`, `I18nStore::tr_vars(&self, key: &str, vars: &[(&str, &str)]) -> String`, `I18nStore::tr_count(&self, key: &str, count: usize) -> String`, `I18nStore::tr_pick(&self, key: &str, seed: usize) -> String` (resolving strings, `{"one","other"}` plural maps, JSON arrays, and automatic `{mac}`/`{Mac}` → `"PC"`).
  - `icons::paint_icon(painter: &egui::Painter, rect: egui::Rect, kind: IconKind, color: Color32)`.

- [ ] **Step 1: Write failing unit tests in `i18n.rs` testing plural objects, array picks, `{Mac}` substitution, and `en`/`vi`/`th` switching (Review Focus #1)**
- [ ] **Step 2: Run `cargo test -p station-ui` to verify FAIL**
- [ ] **Step 3: Implement `theme.rs`, `ui_bold` registration in `lib.rs`, `icons.rs`, and full `I18nStore` in `i18n.rs`**
- [ ] **Step 4: Run `cargo test -p station-ui` to verify PASS**
- [ ] **Step 5: Commit** (`feat(ui): apply exact base.css tokens, bold font family, vector icons, and plural/array i18n`)

---

### Task 2: 3-Column Control-Room Shell (`TopBar` 56px + `SessionRail` 330px + Cropped `Stage` + `ActivityPanel` 350px)

**Files:**
- Create: `crates/station-ui/src/shell/{mod.rs,topbar.rs,session_rail.rs,activity_panel.rs}`
- Delete/Replace: `crates/station-ui/src/{hud.rs,launcher_dock.rs}`
- Modify: `crates/station-app/src/bridge.rs` (sync `StageViewportRect` → `Camera.viewport`)

**Interfaces:**
- Produces:
  - `#[derive(Resource, Debug, Clone, Copy, Default)] pub struct StageViewportRect(pub Option<egui::Rect>);`
  - `pub fn sync_camera_viewport_to_stage(stage_rect: Res<StageViewportRect>, windows: Query<&Window>, mut cams: Query<&mut Camera, With<MainOfficeCamera>>)` (converts logical `egui::Rect` × `scale_factor()` to physical `bevy::render::camera::Viewport`, clamping `>= 1×1` and disabling render when width/height `< 4` px!).
  - Exact `TopBar` (`56px`: logo + title + clock + `DEMO`/`REPLAY` chip + `3D|2D` toggle + native `Apps` harness launcher pill/popover + `Plan` quota pill + `PC/RAM` pill + `Chat` + `Replay` + `Settings` popover + `Phone` + `Night`), `SessionRail` (`330px`: live status, `work`/`wait`/`rest` filter tiles, search box, session cards + nested `ri-subs` subagent list; responsive `< 1100px` collapse), and `ActivityPanel` (`350px`: 5 KPIs, Timeline & Wrapped launchers, `All | Attention`, `Cmd | Edit | Read | Web | Agent` chips, live feed).

- [ ] **Step 1: Write failing unit tests for `StageViewportRect` physical pixel clamping (including `0×0` minimized window!) and responsive `< 1100px` / `< 720px` panel collapse rules (Review Focus #2)**
- [ ] **Step 2: Run `cargo test -p station-ui` and `cargo test -p station-app` to verify FAIL**
- [ ] **Step 3: Implement `topbar.rs`, `session_rail.rs`, `activity_panel.rs`, and `sync_camera_viewport_to_stage`**
- [ ] **Step 4: Run tests + capture `--capture-scene hud-pro --lang vi` PNG and compare layout side-by-side with `my-station-latest-check.png` via `read_file`**
- [ ] **Step 5: Commit** (`feat(ui): build 1:1 3-column control-room shell and cropped 3D camera viewport`)

---

### Task 3: `CameraToolbar`, Stage Overlays (`FirstPersonHud`, `ReplayBar`, `PhotoBar`, `PlaceCard`) & 3D Projected Labels

**Files:**
- Create: `crates/station-ui/src/shell/{camera_toolbar.rs,stage_overlays.rs}`
- Create: `crates/station-ui/src/labels3d.rs`

**Interfaces:**
- Produces:
  - `CameraToolbar` centered `16px` above stage bottom (`[Home | Desks | Meeting | Server]` hover-expanding buttons + `[- 100% +]` zoom + `[Fullscreen | Photo | List]` + solid coral `[Walk in]` button matching `stage.css:11-35`).
  - `pub fn declutter_screen_labels(candidates: &mut [ProjectedLabel], bounds: egui::Rect) -> Vec<ProjectedLabel>` (sorts by priority: active permission > hovered > working session > subagent > zone chip; drops overlapping lower-priority boxes).
  - `FirstPersonHud` (crosshair, help bar, `E` near-action pill, `bil-hud` billiards power bar), `ReplayBar` (scrubber, heatmap ticks, play/speed), `PhotoBar` (Screen / FHD / 4K PNG + 15s Timelapse clip trigger), and `PlaceCard`.

- [ ] **Step 1: Write failing unit tests in `labels3d.rs` proving behind-camera points are dropped and overlapping lower-priority labels are culled while keeping urgent permission badges visible (Review Focus #3)**
- [ ] **Step 2: Run `cargo test -p station-ui labels3d` to verify FAIL**
- [ ] **Step 3: Implement `camera_toolbar.rs`, `stage_overlays.rs`, and `labels3d.rs` wired to `Camera::world_to_viewport`**
- [ ] **Step 4: Run `cargo test -p station-ui` and inspect captured stage PNG via `read_file`**
- [ ] **Step 5: Commit** (`feat(ui): add 1:1 CameraToolbar, stage HUD bars, and decluttered 3D floating labels`)

---

### Task 4: Full `2D Lite` Center Stage Board (`lite.js`) & Standalone `/panel` View (`src/panel/*.js`)

**Files:**
- Modify: `crates/station-ui/src/lite_view.rs`
- Create: `crates/station-ui/src/panel_view.rs`
- Modify: `crates/station-app/src/cli.rs` (add `--panel` compact window mode `420×640`)

**Interfaces:**
- Produces:
  - `render_lite_stage_center(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore)` (renders inside central column between `SessionRail` and `ActivityPanel`: grouped `Working`, `Waiting`, `Resting` sections, git branch/dirty badge, cache-warmth indicator, `done/total` todos, context `%` bar, subagents, keyboard navigation `Up`/`Down`/`Enter`/`Y`/`N`/`/`).
  - `render_compact_panel_window(...)` matching `src/panel/{main,session-card,quota,mac,keys}.js`.

- [ ] **Step 1: Write failing tests for keyboard navigation (`/` focus search, `Y`/`N` only when search unfocused) and status grouping (Review Focus #4)**
- [ ] **Step 2: Run `cargo test -p station-ui` to verify FAIL**
- [ ] **Step 3: Implement `lite_view.rs` and `panel_view.rs`**
- [ ] **Step 4: Capture `--capture-scene lite-2d --lang vi` and visually verify side-by-side against `d:\mystation\my-station\docs\debug\my-station-latest-check.png` via `read_file`**
- [ ] **Step 5: Commit** (`feat(ui): port full 2D Lite center board and compact /panel view with keyboard shortcuts`)

---

### Task 5: Interactive Approval Stack (`Permission`, `AskUserQuestion`, `ExitPlanMode`, `Denied`, `Stuck`, `CI`) & `SessionDrawer`

**Files:**
- Create: `crates/station-ui/src/drawers/{mod.rs,session.rs}`
- Modify: `crates/station-ui/src/shell/stage_overlays.rs`

**Interfaces:**
- Produces:
  - Interactive cards for all 6 agent prompts/alerts (`permissions.js`, `denied-card.js`, `stuck.js`, `ci-card.js`, multi-select/single-select `AskUserQuestion`, markdown plan viewer + feedback box for `ExitPlanMode`).
  - Full `SessionDrawer` (`session-drawer.js` + `session-compose.js` + `pins.js` + `undo-view.js` + `session-account.js`): markdown conversation log (`/api/log`), `Steps | Subagents | Pins | Undo` tabs, prompt compose box with real-time `PromptCoach` tips, `✨ Rewrite` button, `Steer` / `Stop`, account switcher, and `Open IDE Window` (`O`).

- [ ] **Step 1: Write failing unit tests for `AskUserQuestion` option selection payload formatting and `SessionDrawer` tab/pin/undo action emission (Review Focus #5)**
- [ ] **Step 2: Run `cargo test -p station-ui` to verify FAIL**
- [ ] **Step 3: Implement all 6 interactive prompt/alert cards and `drawers/session.rs` using `pulldown-cmark`**
- [ ] **Step 4: Run `cargo test -p station-ui` to verify PASS**
- [ ] **Step 5: Commit** (`feat(ui): implement all 6 approval/question/plan/alert cards and full SessionDrawer`)

---

### Task 6: Full `GroupChat`, `Whiteboard`, `Photo/Timelapse` & `Wrapped` 1080×1350 PNG Export

**Files:**
- Create: `crates/station-ui/src/drawers/{group_chat.rs,whiteboard.rs,modals.rs}`

**Interfaces:**
- Produces:
  - `GroupChat` drawer (`group-chat-view.js`): multi-agent threads, avatar badges (`chat-avatars.js`), emoji `REACTIONS`, reply-to quote, message recall, polls, reminders, stickers, image attachment preview (`/api/image`), `@agent` mentions.
  - `Whiteboard` modal (`whiteboard.js`): draggable/editable sticky notes & todo checkboxes synced to `/api/todos` and re-rasterizing the 3D wall whiteboard texture.
  - `Wrapped` modal (`wrapped.js`): fetches `/api/wrapped`, previews and exports a crisp `1080×1350` shareable PNG card via `station-canvas2d` + copies to Windows clipboard.
  - `Photo` (`Screen`, `1920×1080`, `3840×2160` PNG with optional nametags/watermark) & `15s Timelapse` recorder (frame sequence → animated GIF / MP4 + procedural `clip-music.js` synth).

- [ ] **Step 1: Write failing tests for `GroupChat` reaction/poll actions and `1080×1350` `Wrapped` PNG dimensions**
- [ ] **Step 2: Run `cargo test -p station-ui` to verify FAIL**
- [ ] **Step 3: Implement `group_chat.rs`, `whiteboard.rs`, and `modals.rs` (`Wrapped`, `Photo`, `Timelapse`)**
- [ ] **Step 4: Run `cargo test -p station-ui` to verify PASS**
- [ ] **Step 5: Commit** (`feat(ui): port full GroupChat, sticky-note Whiteboard, Photo/Timelapse, and 1080x1350 Wrapped`)

---

### Task 7: `BurnDrawer`, `VaultDrawer` + Prompt Report, `MacDrawer` + Stranded, `AI Accounts` Popover & `PhoneModal`

**Files:**
- Create: `crates/station-ui/src/drawers/{burn.rs,vault.rs,mac_sys.rs,accounts.rs}`
- Modify: `crates/station-ui/src/drawers/modals.rs` (`PhoneModal`)

**Interfaces:**
- Produces:
  - `BurnDrawer` (`burn.js`): `Today | 7d | 30d` tabs, stacked daily cost/token bar chart, per-model / MCP / skill / plugin / subagent / role / project breakdowns.
  - `VaultDrawer` (`vault.js`): accent-insensitive live search, full transcript reader modal, 1-click copy `claude --resume <id>`, and `Prompt Coach Weekly Report` tab (`/api/vault/report`).
  - `MacDrawer` (`mac-drawer.js` + `stranded.js`): CPU/RAM/swap/disk gauges, top process memory table, 2-click safe cleanup/kill buttons, idle auto-close hours `[Off, 2h, 4h, 8h]`, and Stranded uncommitted/unpushed git worktrees with `Open` / `Remove`.
  - `AI Accounts` TopBar popover (`ai-accounts.js`): `Claude | Codex | Antigravity` tabs, 5h/7d quota bars, add/login/rename/use/remove/auto-switch/reorder.
  - `PhoneModal`: rasterized QR code image (`draw_qr_block` on LAN IP / Cloudflare tunnel URL — never `127.0.0.1`!), real PIN view/edit/regenerate, LAN toggle, Cloudflare quick/custom-domain tunnel toggle, phone-approve toggle, and Web Push test button.

- [ ] **Step 1: Write failing unit tests for Burn chart bucket scaling, Phone LAN URL selection (preferring private LAN IPv4 over `127.0.0.1`), and AI Account tab state**
- [ ] **Step 2: Run `cargo test -p station-ui` to verify FAIL**
- [ ] **Step 3: Implement `burn.rs`, `vault.rs`, `mac_sys.rs`, `accounts.rs`, and `PhoneModal`**
- [ ] **Step 4: Run `cargo test -p station-ui` to verify PASS**
- [ ] **Step 5: Commit** (`feat(ui): port Burn charts, Vault + Prompt Report, Mac/Stranded, AI Accounts, and Phone QR modal`)

---

### Task 8: Zero-Hardcoded-English Audit & V1 Side-by-Side Visual Gate

**Files:**
- Modify: `PARITY_LEDGER.md`

- [ ] **Step 1: Audit `crates/station-ui/src` for any remaining hardcoded English UI literals; capture `--capture-scene hud-pro` and `--capture-scene lite-2d` in `en`, `vi`, and `th`; inspect all 6 PNGs via `read_file` against `my-station` screenshots**
- [ ] **Step 2: Verify `cargo test --workspace` passes and 2D Lite Private WS stays `≤ 110 MB`**
- [ ] **Step 3: Mark all 42 `src/office/ui/`, 7 `src/panel/`, 20 `src/components/`, 55 `src/styles/`, and 105 `src/i18n/` rows `[x]` in `PARITY_LEDGER.md` and commit** (`docs(v1): certify 1:1 control-room UI shell, drawers, and en/vi/th i18n parity`)
