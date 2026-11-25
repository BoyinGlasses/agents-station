# Agents Station — Plan R1: Security, Persistence, Win32 Tray & View Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate all security vulnerabilities (cross-port CSRF RCE, DNS rebinding, tunnel spoof, `/api/replay` traversal, PIN brute-force, `/phone` XSS, unredacted chat/denied payloads, Win32 UB), reset falsified `PARITY_LEDGER.md` entries, make persistence crash-safe, and fix the broken System Tray (`≤ 45 MB`) and `3D ↔ 2D Lite` lifecycle.

**Architecture:** Harden `station-core::http::guard` with exact-port `Origin`, universal `Host`, and socket-level `ViaTunnel` checks; replace Windows file-overwrite and TCP/process Win32 calls with UB-free, crash-atomic Win32 primitives; split parent Tray/HTTP daemon (`agents-station` host, `~15 MB`) from on-demand `--gui-worker` Bevy renderer while keeping `LiteCamera` independent of `WorldEntity` so both `Tray` (`≤ 45 MB`) and `3D ↔ 2D Lite` work without losing `:4310` or stranding zombie processes.

**Tech Stack:** Rust 1.97+, `axum` `0.8`, `tokio`, `windows-sys` `0.59`, `bevy` `0.16`, `bevy_egui` `0.34`.

**Spec:** `d:\mystation\agents-station\docs\superpowers\specs\2026-11-25-remediation-and-1to1-visual-spec.md`

## Global Constraints

- Single Compiler Lock: NEVER run two concurrent `cargo` commands (`target-dir = "D:/mystation/cargo-target"`, `jobs = 2`).
- NEVER modify files in `d:\mystation\my-station` (read-only Golden Oracle).
- Zero `powershell.exe` or console window flashes (`CREATE_NO_WINDOW = 0x08000000` on every background subprocess).
- Tray mode total Private Working Set MUST be `≤ 45 MB` with `:4310` remaining live.
- Every task ends with passing tests and an atomic Conventional Commit in `d:\mystation\agents-station`.

## Review Focus

1. **Cross-port localhost CSRF (`Origin: http://localhost:5173` → `:4310`)** — browser POSTs from another port on `localhost` or `127.0.0.1` must receive `403` on `/event`, `/hook/*`, `/v1/*`, `/api/harnesses/*`, and `/api/*`; pinned by tests in Task 2.
2. **DNS rebinding (`Host: attacker.com:4310` from `127.0.0.1`) & spoofed `x-office-via-tunnel` header** — mismatched `Host` must return `421` before `is_loopback_peer` grants local trust, and tunnel status must derive only from a server-side `ViaTunnel` extension; pinned by tests in Task 2.
3. **`/api/replay?day=../../...` traversal, PIN brute-force, & non-Claude `codex:123` Undo failure** — non-`YYYY-MM-DD` replay days must return `400`, 6th bad PIN from an IP must return `429`, and `UndoManager` must back up & revert files for `codex:abc` while rejecting symlinks and paths outside `cwd`; pinned by tests in Tasks 3 & 4.
4. **Data loss on Windows `TerminateProcess` or locked destination file during atomic save** — `MoveFileExW(..., MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` must overwrite existing JSON atomically without `remove_file`, and `vault`/`trophies`/`tenure`/15s `otel` + `replay/YYYY-MM-DD.jsonl` must survive abrupt kill; pinned by tests in Task 5.
5. **Clicking `Tray` or `X` stranding an invisible zombie, or `3D → 2D → 3D` losing the camera / scrambling camera on `WASD` typing** — Tray must keep a clickable Win32 `NOTIFYICONDATAW` + wake on second launch at `≤ 45 MB`, `3D → 2D → 3D` must preserve an active camera and rebuild `WorldEntity`s, and typing `f`/`wasd` in `egui` must not move `CameraRig`; pinned by tests in Tasks 6, 7, & 8.

---

## File Structure

- Modify: `PARITY_LEDGER.md` — reset stubbed/unwired rows from `[x]` to `[ ]` / `[~]` with honest status headers.
- Modify: `crates/station-core/src/http/guard.rs` — universal `Host` check (`421`), exact-port `Origin` check, `ViaTunnel` extension.
- Modify: `crates/station-core/src/http/{mod.rs,api.rs,hooks.rs,phone.rs}` — replay `day` validation, PIN rate-limiter, `phoneApprove` + `danger` check on `/api/perm`, `/phone` HTML escaping, `redact()` on `post_chat` & `post_hook_denied`, `.unwrap_or_else(|e| e.into_inner())` mutex recovery.
- Modify: `crates/station-launcher/src/lib.rs` — validate custom harness `.exe`/`.lnk` existence, reject UNC/URI, auto-fill `process_names`.
- Modify: `crates/station-core/src/platform/win32.rs`, `crates/station-core/src/system.rs` — `Vec<u32>` aligned TCP table buffer, `GetExitCodeProcess == 259`, `descendants` visited set.
- Modify: `crates/station-core/src/state/undo.rs` — support `<agent>:<sid>`, sanitize `:` → `_` on disk, `symlink_metadata`, `cwd` boundary check, 50-session eviction.
- Modify: `crates/station-core/src/atomic.rs`, `crates/station-core/src/state/sessions.rs` — `MoveFileExW` atomic replace, `data/replay/YYYY-MM-DD.jsonl` append, 15s dirty-flush & 60s session/perm/undo reaper task.
- Modify: `crates/station-app/src/{cli.rs,commands.rs,instance.rs,tray.rs,main.rs,bridge.rs,fps.rs}` — exe-anchored `data_dir`, `:4310` daemon attach vs spawn, persistent Win32 `TrayHost` + `--gui-worker` child lifecycle (`≤ 45 MB` tray), `LiteCamera` + `rebuild_office_world` for `3D ↔ 2D Lite` & `SwitchLayout`, `0 FPS` when minimized.
- Modify: `crates/station-3d/src/{lib.rs,camera.rs}` — untag camera or manage `OfficeCamera` cleanly, `egui` input-capture guard on `camera_control_system`.

---

### Task 1: Honest `PARITY_LEDGER.md` Reset & Ledger Audit Test

**Files:**
- Modify: `PARITY_LEDGER.md`
- Modify: `crates/station-core/src/lib.rs`

**Interfaces:**
- Produces: Honest `[x]` (verified real), `[~]` (partial/stubbed), `[ ]` (unported) markers across all 571 items in `PARITY_LEDGER.md`; `station_core::tests::parity_ledger_tracks_every_source_file_without_omissions` updated to count `- [x]`, `- [~]`, and `- [ ]`.

- [ ] **Step 1: Update ledger test in `crates/station-core/src/lib.rs` to assert total tracked rows `>= 571` while distinguishing `[x]` vs `[~]`/`[ ]`**
- [ ] **Step 2: Reset all stubbed HTTP routes (31), dead backend modules (19), missing canvas surfaces, unported 3D rooms/props/shops/street/people/life files, and stubbed UI/i18n rows in `PARITY_LEDGER.md` to `[~]` or `[ ]`**
- [ ] **Step 3: Run `cargo test -p station-core` and verify PASS**
- [ ] **Step 4: Commit** (`docs(parity): reset falsified ledger rows to honest verified status`)

---

### Task 2: HTTP Security Hardening (Exact-Port `Origin`, Universal `Host`, `ViaTunnel` & Custom Exe Guard)

**Files:**
- Modify: `crates/station-core/src/http/guard.rs`
- Modify: `crates/station-core/src/http/{mod.rs,api.rs,hooks.rs,otel_routes.rs}`
- Modify: `crates/station-launcher/src/lib.rs`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Copy, Default)] pub struct ViaTunnel(pub bool);`
  - `pub fn enforce_host_allowed(headers: &HeaderMap, allowed_tunnel_host: Option<&str>) -> Result<(), StatusCode>` (returns `StatusCode::MISDIRECTED_REQUEST` `421` on unknown `Host`)
  - `pub fn enforce_hook_loopback(peer: Option<SocketAddr>, via_tunnel: bool, headers: &HeaderMap) -> Result<(), StatusCode>` (rejects non-loopback, `via_tunnel`, invalid `Host`, and **any** `Origin` header with `403`)
  - `pub fn enforce_same_origin_post(headers: &HeaderMap, allowed_tunnel_host: Option<&str>) -> Result<(), StatusCode>` (requires `Origin` authority + port to match `Host` exactly when `Origin` is present)
  - `pub fn validate_custom_executable_path(raw: &str) -> anyhow::Result<(PathBuf, Vec<String>)>` in `station-launcher` (rejects `\\` UNC, `://` URIs, non-`.exe`/`.lnk` extensions, and missing files; extracts `vec![file_name]` as default `process_names`).

- [ ] **Step 1: Write failing unit tests in `guard.rs` and `station-launcher/src/lib.rs` (Review Focus #1 & #2)**

Assert:
- `POST /event` with `Origin: http://127.0.0.1:4310` → `403` (hooks never come from browser `Origin`).
- `POST /api/harnesses/custom` with `Host: 127.0.0.1:4310` and `Origin: http://localhost:5173` → `403`!
- `GET /api/live` from `127.0.0.1` with `Host: evil.com:4310` → `421`!
- `GET /api/live` from `127.0.0.1` with `ViaTunnel(true)` and no PIN cookie → `401` (even without `x-office-via-tunnel` header!).
- `validate_custom_executable_path("\\\\evil\\share\\p.exe")`, `"cmd.exe"` (non-absolute/missing), and `"https://evil.com"` return `Err`; an existing temp `.exe` returns `Ok((path, vec!["temp.exe".into()]))`.

- [ ] **Step 2: Run `cargo test -p station-core http::guard` and `cargo test -p station-launcher` to verify FAIL**
- [ ] **Step 3: Implement guards in `guard.rs`, apply across `mod.rs`/`api.rs`/`hooks.rs`/`otel_routes.rs`, and enforce `validate_custom_executable_path` in `station-launcher`**
- [ ] **Step 4: Run `cargo test -p station-core` and `cargo test -p station-launcher` to verify PASS**
- [ ] **Step 5: Commit** (`fix(security): enforce exact-port Origin, Host 421, ViaTunnel, and local exe validation`)

---

### Task 3: Replay Traversal Fix, PIN Rate Limiter, `/phone` XSS Escape & Complete Redaction

**Files:**
- Modify: `crates/station-core/src/auth.rs`
- Modify: `crates/station-core/src/http/{api.rs,hooks.rs,phone.rs}`

**Interfaces:**
- Produces:
  - `pub fn is_valid_day_key(day: &str) -> bool` (`^\d{4}-\d{2}-\d{2}$`)
  - `pub struct PinRateLimiter` (`check_and_record(&mut self, ip: IpAddr, now_ms: u64, success: bool) -> Result<(), StatusCode>` — 5 failures / 15 min per IP, 20 failures / 15 min global → `429`)
  - `pub fn redact_json_value(v: &Value) -> Value` (recursively applies `redact()` to all JSON string values)
  - `pub fn escape_html(s: &str) -> String`

- [ ] **Step 1: Write failing tests for `?day=../../secret`, PIN lockout on 6th failure, `/api/perm` `phoneApprove`/`danger` block, `/phone` `<img onerror>` escaping, and `post_hook_denied` / `post_chat` redaction (Review Focus #3)**
- [ ] **Step 2: Run `cargo test -p station-core` to verify FAIL**
- [ ] **Step 3: Implement `is_valid_day_key`, `PinRateLimiter` on `AppState`, `phoneApprove` + `danger` check in `post_perm`, `textContent` / `escape_html` in `phone.rs`, and `redact` in `post_chat` + `post_hook_denied`**
- [ ] **Step 4: Run `cargo test -p station-core` to verify PASS**
- [ ] **Step 5: Commit** (`fix(security): block replay traversal, add PIN lockout, escape phone HTML, and redact chat/denied`)

---

### Task 4: Win32 Alignment UB / Zombie PID Fix & Multi-Agent Scoped `UndoManager`

**Files:**
- Modify: `crates/station-core/src/platform/win32.rs`
- Modify: `crates/station-core/src/system.rs`
- Modify: `crates/station-core/src/state/undo.rs`

**Interfaces:**
- Produces:
  - Aligned `Vec<u32>` buffer in `collect_tcp_v4` / `collect_tcp_v6` + `GetExitCodeProcess == 259` check in `is_pid_alive` + cycle-safe `descendants` (`HashSet<u32>`).
  - `UndoManager::before(&mut self, sid: &str, cwd: Option<&Path>, tool: &str, file_path: &Path)` accepting `codex:s1` / `antigravity:c1` (`sid_disk_key` mapping `:` → `_`), skipping symlinks (`symlink_metadata`), rejecting paths outside `cwd`, and evicting oldest beyond `50` sessions.

- [ ] **Step 1: Write failing unit tests in `undo.rs` (`codex:sess-1` round-trip revert, symlink skip, `../outside` rejection, 51st session eviction) and `system.rs` (`descendants` cycle guard) (Review Focus #3)**
- [ ] **Step 2: Run `cargo test -p station-core undo` and `cargo test -p station-core system` to verify FAIL**
- [ ] **Step 3: Implement fixes in `win32.rs`, `system.rs`, and `undo.rs`**
- [ ] **Step 4: Run `cargo test -p station-core` to verify PASS**
- [ ] **Step 5: Commit** (`fix(core): fix Win32 TCP UB and zombie PIDs, support non-Claude scoped undo`)

---

### Task 5: Crash-Safe `MoveFileExW` Persistence, `replay/*.jsonl` Writer, Reaper Tick & Poison-Safe Locks

**Files:**
- Modify: `crates/station-core/src/atomic.rs`
- Modify: `crates/station-core/src/http/{mod.rs,hooks.rs,otel_routes.rs}`
- Modify: `crates/station-core/src/state/sessions.rs`
- Modify: `crates/station-core/src/store/{otel.rs,wrapped.rs}`

**Interfaces:**
- Produces:
  - `write_json_atomic` using Win32 `MoveFileExW(..., MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` (zero `remove_file` window).
  - Immediate mutation persistence on `vault`, `trophies`, `tenure`; append-only `append_replay_event(data_dir, now_ms, &compact_event)` writing `data/replay/YYYY-MM-DD.jsonl` in `Local` timezone.
  - `pub fn spawn_maintenance_loop(state: Arc<AppState>, shutdown: watch::Receiver<bool>)` flushing dirty `OtelStore` every `15s` and reaping dead/idle sessions (`autoCloseHours`), answered/expired `pending_perms`, and stale `undo` entries every `60s`.
  - `Local` timezone fix in `wrapped.rs` + `"30d"` window in `otel.rs` + `.unwrap_or_else(|e| e.into_inner())` helper macro/fn across `station-core`.

- [ ] **Step 1: Write failing tests proving: (a) `OtelStore`, `VaultStore`, `TrophyStore`, and `data/replay/YYYY-MM-DD.jsonl` persist to disk BEFORE server shutdown (`kill` simulation), (b) `"30d"` burn range returns 30 days, and (c) idle sessions reap past `autoCloseHours` (Review Focus #4)**
- [ ] **Step 2: Run `cargo test -p station-core` to verify FAIL**
- [ ] **Step 3: Implement `MoveFileExW` in `atomic.rs` (`Win32_Storage_FileSystem` already in `Cargo.toml`), replay JSONL writer, mutation saves, `spawn_maintenance_loop` in `start_http_server`, and timezone/`30d` fixes**
- [ ] **Step 4: Run `cargo test -p station-core` to verify PASS**
- [ ] **Step 5: Commit** (`fix(core): add MoveFileExW atomic replace, live flush/replay writer, and session reaper`)

---

### Task 6: Exe-Anchored `data_dir`, `:4310` Service Attach Client & Persistent Win32 `TrayHost` (`≤ 45 MB`)

**Files:**
- Modify: `crates/station-app/src/{cli.rs,commands.rs,instance.rs,tray.rs,main.rs,bridge.rs}`

**Interfaces:**
- Produces:
  - `Cli` flag `#[arg(long, hide = true)] pub gui_worker: bool`
  - `pub fn resolve_install_root() -> PathBuf` (anchors default `data/` to repo root or `current_exe()` parent, never arbitrary `CWD`)
  - `pub struct TrayDaemon` in `tray.rs`: hidden Win32 window class `"AgentsStationTrayHost"`, real 16×16/32×32 ARGB terracotta icon (`CreateIconIndirect`), persistent `Shell_NotifyIconW(NIM_ADD)` (`NIF_ICON | NIF_MESSAGE | NIF_TIP`, **no immediate `NIM_DELETE`**!), `WM_RBUTTONUP` popup menu (`Open 3D Office`, `Open 2D Lite`, `Quit`), left-click / named-event (`Local\agents-station-show`) spawning or focusing `--gui-worker` (`CREATE_NO_WINDOW`), and clean `NIM_DELETE` + `flush_all()` on `Quit`.
  - When `--gui-worker` runs (or standalone `--capture-scene`): `LiveServerBridgePlugin` connects to the parent/service `:4310` over local HTTP + `/stream` SSE if `:4310/health` is already up, or starts in-process HTTP if standalone. Closing window (`X`) or clicking `Tray` exits only `--gui-worker`, dropping total resident RAM to the parent `TrayDaemon + :4310` process (`~8–22 MB`, well below `≤ 45 MB`!).

- [ ] **Step 1: Write failing tests in `tray.rs`, `instance.rs`, and `commands.rs` verifying: (a) `resolve_runtime_data_dir` ignores unrelated temp `CWD`, (b) `TrayDaemon` creates a valid Win32 `HICON` and message HWND without deleting its icon, and (c) signaling `Local\agents-station-show` wakes the listener (Review Focus #5)**
- [ ] **Step 2: Run `cargo test -p station-app` to verify FAIL**
- [ ] **Step 3: Implement `TrayDaemon`, parent/worker split (`agents-station` default GUI launches `TrayDaemon` + `:4310` + `--gui-worker`), `WindowCloseRequested` handling, and SSE/HTTP client bridge when `:4310` is already bound**
- [ ] **Step 4: Run `cargo test -p station-app` and measure real Private Working Set after closing `--gui-worker` to Tray (`Win32_PerfRawData_PerfProc_Process` MUST be `≤ 45 MB`, `:4310/health` MUST return `200 "ok"`)**
- [ ] **Step 5: Commit** (`fix(app): implement persistent Win32 TrayDaemon (<25MB) and service-attach bridge`)

---

### Task 7: Working `3D ↔ 2D Lite` Rebuild, Layout Switch, `egui` Input Guard & `0 FPS` Minimize

**Files:**
- Modify: `crates/station-3d/src/{lib.rs,camera.rs,palette.rs,backdrops.rs}`
- Modify: `crates/station-app/src/{main.rs,bridge.rs,fps.rs}`

**Interfaces:**
- Produces:
  - `#[derive(Component)] pub struct Lite2dCamera;` (never tagged `WorldEntity`)
  - `pub fn spawn_full_office_world(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, fonts: &mut FontEngine, assets_dir: &Path, layout: OfficeLayout, backdrop: BackdropKind, look: ChibiLook, night: bool)`
  - `pub fn unload_full_office_world(...)` (despawns `WorldEntity`s AND removes `WorldPalette` meshes/materials/images + `BackdropAssets` materials/images — fixing the 14-material leak on backdrop switch!)
  - `camera_control_system` taking `mut egui_ctx: bevy_egui::EguiContexts` and returning early when `ctx.wants_keyboard_input()` or `ctx.wants_pointer_input()` is true.
  - `fps.rs` checking `win.is_minimized()` / `WindowOccluded` → `0 FPS` (`Duration::from_millis(500)` idle sleep).

- [ ] **Step 1: Write failing tests in `station-3d` & `station-app` proving: (a) `3D → Lite2D → Mode3D → SwitchLayout(Hq)` leaves exactly 1 active `Camera3d` at every step and restores `WorldEntity` count > 0 on return to 3D, (b) 3× backdrop switch leaks `0` `StandardMaterial`s and `0` `Image`s, and (c) `PbrPlugin` stays active so `--lite` can switch to 3D without error (Review Focus #5)**
- [ ] **Step 2: Run `cargo test -p station-3d` and `cargo test -p station-app` to verify FAIL**
- [ ] **Step 3: Implement `spawn_full_office_world` / `unload_full_office_world`, wire `SetViewMode(Mode3D)`, `SwitchLayout`, `SetChibiLook` in `bridge.rs`, add `egui` input guard in `camera.rs`, share 1 `FontEngine` resource across switches, and add minimize `0 FPS` in `fps.rs`**
- [ ] **Step 4: Run `cargo test --workspace` to verify PASS**
- [ ] **Step 5: Commit** (`fix(3d,app): fix 3D<->2D camera/world rebuild, material leaks, egui input hijack, and minimize FPS`)

---

### Task 8: Unused Crate Cleanup & R1 End-to-End Security/Lifecycle Gate

**Files:**
- Modify: `Cargo.toml`, `crates/station-core/Cargo.toml`
- Modify: `PARITY_LEDGER.md`

- [ ] **Step 1: Remove unused `notify` and `tower-http` dependencies from `Cargo.toml` and `crates/station-core/Cargo.toml`**
- [ ] **Step 2: Run `cargo test --workspace` + verify Tray Private WS `≤ 45 MB` + verify cross-port `Origin: http://localhost:5173` returns `403` + verify `Host: evil.com` returns `421`**
- [ ] **Step 3: Update `PARITY_LEDGER.md` for completed R1 security/persistence/tray items and commit** (`chore(r1): drop unused deps and certify R1 security, persistence, and tray gate`)
