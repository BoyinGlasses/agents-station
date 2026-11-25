# Agents Station — Plan R2: Blocking Hooks, Full Backend Wiring, Live 3D Bridge & Audio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every remaining stubbed HTTP route (`31+`), wire all `19` dead `station-core` modules, implement real blocking `oneshot` `/hook/{permission,question,denied}` + Windows Toast approvals, connect live `OfficeState` sessions/events to 3D workers/screens/picking, enable real 27-SFX audio output, and certify backend parity against the live `my-station` Golden Oracle.

**Architecture:** `OfficeState` holds `oneshot::Sender<Value>` channels governed by `mac_hold::hold_plan` + `X-Office-Wait`. `AppState` integrates all 19 previously unwired workflow/account/history modules behind `spawn_blocking` for heavy I/O. `LiveServerBridge` streams real events into `UiState` and `station-3d` (spawning/moving `ChibiWorker`s, swapping `ScreenMaterials`, firing `bevy_picking` observers, and playing WAV buffers via Win32 `PlaySoundW`).

**Tech Stack:** Rust 1.97+, `axum`, `tokio`, `p256` + `aes-gcm` + `hkdf` (RFC 8291/8292 Web Push), `flate2` (gzip vault), `rusqlite` (optional or binary SQLite reader for Antigravity `state.vscdb`), `windows-sys` (`Win32_Media_Audio` `PlaySoundW`).

**Spec:** `d:\mystation\agents-station\docs\superpowers\specs\2026-11-25-remediation-and-1to1-visual-spec.md`

## Global Constraints

- Prerequisite: Plan R1 (`2026-11-25-plan-r1-security-persistence-and-lifecycle.md`) completed and green.
- Single Compiler Lock (`target-dir = "D:/mystation/cargo-target"`, `jobs = 2`).
- Zero `post_local_ok`, `get_generic_ok`, `post_accounts_any`, or `=> {}` `UiAction` stubs may exist at the end of Plan R2.
- All child CLI processes (`claude`, `codex`, `opencode`, `git`, `cloudflared`) MUST set `.creation_flags(0x08000000)` (`CREATE_NO_WINDOW`) on Windows.
- `serve` idle Private WS MUST remain `≤ 35 MB` with `0` `powershell.exe` spawns.

## Review Focus

1. **`/hook/permission` & `/hook/question` hanging forever when nobody is at the desk or timing out past `X-Office-Wait`** — when `hold_plan` returns `ms == 0` (unfocused, away, no phone push), hook must return `{}` immediately; when focused, it must wait up to `min(wait_header, normal_ms)` and unblock immediately with exact Claude `hookSpecificOutput` JSON when `POST /api/perm` or `UiAction::DecidePermission` fires; pinned by tests in Task 1.
2. **Maximized IDE window shrinking when clicked in `HarnessLauncherDock`, or 5×/sec process scan stalling Bevy frames** — launcher polling must run on a background thread once per second, and `focus_window` must only call `ShowWindow(SW_RESTORE)` when `IsIconic(hwnd) != 0`; pinned by tests in Task 2.
3. **Fake demo sessions (`with_demo_seed`) sticking on screen or `"work"` vs `"working"` mismatch showing `0 working`** — live mode must start with empty sessions/perms, clear last item when empty, and map `"work"`/`"wait"`/`"rest"` consistently across `station-core`, `station-ui`, and `station-3d`; pinned by tests in Task 6.
4. **Web Push throwing `InvalidAccessError` in browsers due to non-curve random VAPID bytes** — `PushStore` must generate a valid NIST P-256 keypair (`0x04 || X || Y` on curve) and encrypt RFC 8291 `aes128gcm` payloads; pinned by unit test in Task 5.
5. **Oracle schema drift hidden by self-referential contract tests** — `scripts/run-oracle-contract-tests.mjs` must boot BOTH `d:\mystation\my-station` (`OFFICE_NO_NEXT=1`) and `agents-station.exe serve` on ephemeral ports and assert matching status codes and normalized JSON key shapes across endpoints; pinned in Task 8.

---

## File Structure

- Modify: `crates/station-core/src/state/{sessions.rs,mac_hold.rs,tasks.rs}` — `oneshot` hold registry on `PendingPerm`, `resolve_permission`, real `TaskManager` / `BgSessions` / `Steer` CLI spawning.
- Modify: `crates/station-core/src/http/{mod.rs,hooks.rs,api.rs,phone.rs}` — wire all 19 dead modules into `AppState` & replace all 31+ stub routes; full interactive `/phone` HTML + real `/sw.js`.
- Modify: `crates/station-core/src/hooks_cli.rs`, `crates/station-app/src/{cli.rs,commands.rs}` — add `status-line` subcommand & wire `cfg.statusLine` in Claude `settings.json`; wire `/api/models` to `~/.claude/settings.json` + `CLAUDE.md`.
- Modify: `crates/station-core/src/store/{vault.rs,codex_accounts.rs,antigravity_accounts.rs,push.rs}` + Create: `crates/station-core/src/store/{claude_accounts.rs,tunnel.rs,images.rs}` — `~/.claude/projects` `.jsonl` gzip/accent-folded vault, real account managers, P-256 Web Push, `cloudflared` tunnel + dedicated `ViaTunnel(true)` listener, `/api/image` sandboxed store.
- Modify: `crates/station-launcher/src/{focus_launch.rs,lib.rs}` — single-pass background process/HWND cache + `IsIconic` guard.
- Modify: `crates/station-3d/src/{people.rs,desks.rs,interact.rs,audio.rs,lib.rs}` + `Cargo.toml` (`Win32_Media_Audio`) — live session → `ChibiWorker` & `ScreenMaterials` sync, `bevy_picking` pointer observers (`HoverRing`, `PlaceCardState`, desk click → session drawer), Win32 `PlaySoundW` async WAV player.
- Modify: `crates/station-app/src/{main.rs,bridge.rs,tray.rs}` — empty initial live `UiState`, wire all 12 `UiAction`s, Windows Toast notification on unfocused permission hold.
- Modify: `scripts/run-oracle-contract-tests.mjs` — dual-server (`my-station` vs `agents-station`) schema & behavior diff suite.

---

### Task 1: Blocking `/hook/{permission,question,denied}` Hold Queue & Toast/API Resolution

**Files:**
- Modify: `crates/station-core/src/state/{sessions.rs,mac_hold.rs}`
- Modify: `crates/station-core/src/http/{hooks.rs,api.rs}`
- Modify: `crates/station-app/src/{bridge.rs,tray.rs}`

**Interfaces:**
- Produces:
  - `OfficeState::register_hold(&mut self, perm: PendingPerm, tx: oneshot::Sender<Value>)`
  - `OfficeState::resolve_hold(&mut self, id: &str, decision: PermDecision) -> Option<Value>` (`Allow`, `Deny { reason }`, `Answers(Map)`, `Retry`, `Keep`)
  - `post_hook_permission`, `post_hook_question`, `post_hook_denied` awaiting `rx` with `tokio::time::timeout(Duration::from_millis(hold_ms))` computed from `hold_plan(...)` and `X-Office-Wait`.

- [ ] **Step 1: Write failing async tests in `hooks.rs` proving: (a) unfocused server returns `{}` immediately (`< 50 ms`), (b) focused server holds `POST /hook/permission` until `POST /api/perm` (`{ id, choice: "allow" }`) resolves it with Claude `hookSpecificOutput.permissionDecision == "allow"`, and (c) `/hook/denied` holds for retry when open (Review Focus #1)**
- [ ] **Step 2: Run `cargo test -p station-core http::hooks` to verify FAIL**
- [ ] **Step 3: Implement `oneshot` hold map, `resolve_hold`, `/api/perm` + `/api/denied` + `/api/panel/answer` + `UiAction::DecidePermission` resolution, and unfocused tray balloon/toast alert**
- [ ] **Step 4: Run `cargo test -p station-core` to verify PASS**
- [ ] **Step 5: Commit** (`feat(core): implement blocking oneshot hold queue for permission, question, and denied hooks`)

---

### Task 2: Background 1 Hz Launcher Scanner & `IsIconic` Maximize-Preserving Focus

**Files:**
- Modify: `crates/station-launcher/src/{focus_launch.rs,lib.rs}`
- Modify: `crates/station-app/src/bridge.rs`

**Interfaces:**
- Produces:
  - `WindowLauncher::snapshot_running_windows(&self) -> HashMap<String, (u32, isize)>` (1 single `sysinfo` + `EnumWindows` pass skipping owned/tool windows per poll instead of 5× per second!)
  - `focus_window` checking `IsIconic(hwnd) != 0` before calling `ShowWindow(hwnd, SW_RESTORE)`, then `SetForegroundWindow(hwnd)`.

- [ ] **Step 1: Write failing unit tests in `station-launcher` proving `snapshot_running_windows` runs once per `list()` call and `focus_window` does not restore non-minimized HWNDs (Review Focus #2)**
- [ ] **Step 2: Run `cargo test -p station-launcher` to verify FAIL**
- [ ] **Step 3: Implement single-pass snapshot, `IsIconic` check, background polling channel in `bridge.rs`, and accurate `NotInstalled`/`LaunchedNew`/`FocusedExisting` toasts**
- [ ] **Step 4: Run `cargo test -p station-launcher` and `cargo test -p station-app` to verify PASS**
- [ ] **Step 5: Commit** (`perf(launcher): poll running windows once on background thread and preserve maximized state`)

---

### Task 3: Wire Dead Workflow Modules (`stranded`, `git`, `testrun`, `usage`, `timeline`, `tasks`, `bg_sessions`, `steer`, `auto_resume`, `images`)

**Files:**
- Modify: `crates/station-core/src/http/{mod.rs,api.rs,hooks.rs}`
- Modify: `crates/station-core/src/state/{sessions.rs,tasks.rs,git.rs}`
- Create: `crates/station-core/src/store/images.rs`

**Interfaces:**
- Produces:
  - Live `GitTracker` (`CREATE_NO_WINDOW`), `UsageScanner` (populating real `cost_usd` & token counts per session!), `StrandedScanner` (`GET /api/stranded`, `POST /api/stranded/{open,remove}`), `build_timeline` backed by today's `data/replay/YYYY-MM-DD.jsonl` (`GET /api/timeline`), sandboxed `ImageStore` (`GET /api/image` with `CSP: sandbox` + `nosniff`), real `TaskManager` + `BgSessions` + `steer::deliver` + `AutoResumeGuard` (`POST /api/tasks`, `/allow`, `/bg`, `/stop`, `/steer`, `/api/ci`), and real `Rewriter` CLI/API transport (`POST /api/prompt/rewrite`).

- [ ] **Step 1: Write failing route tests for `/api/stranded`, `/api/timeline` (non-empty after `/event`), `/api/image` (`CSP: sandbox`), `/api/tasks` (spawning mock CLI without wedging at 3), and session `cost_usd` from `UsageScanner`**
- [ ] **Step 2: Run `cargo test -p station-core` to verify FAIL**
- [ ] **Step 3: Wire all workflow & store modules into `AppState` (using `spawn_blocking` for git/file scans) and delete `get_generic_ok` / `post_local_ok` from those routes**
- [ ] **Step 4: Run `cargo test -p station-core` to verify PASS**
- [ ] **Step 5: Commit** (`feat(core): wire stranded, git, usage, timeline, image, rewrite, and task/steer engines`)

---

### Task 4: Claude `statusLine`, `~/.claude/settings.json` Models, Real `~/.claude/projects` Vault & Multi-Account Managers

**Files:**
- Modify: `crates/station-core/src/hooks_cli.rs`
- Modify: `crates/station-core/src/store/{vault.rs,codex_accounts.rs,antigravity_accounts.rs}`
- Create: `crates/station-core/src/store/claude_accounts.rs`
- Modify: `crates/station-core/src/http/{mod.rs,api.rs}`
- Modify: `crates/station-app/src/{cli.rs,commands.rs}`

**Interfaces:**
- Produces:
  - `Command::StatusLine` CLI subcommand forwarding existing `statusline-prev.json` + POSTing `/hook/status`, installed by `install_claude`.
  - `/api/models` reading/writing `~/.claude/settings.json` + `~/.claude/CLAUDE.md` via `state::models::{models_of, with_models, with_split}`.
  - `VaultStore::scan_claude_projects(&mut self, claude_dir: &Path)` indexing `~/.claude/projects/**/*.jsonl` with gzip transcript archive, `<system-reminder>` strip, and Vietnamese accent-insensitive `fold()` search.
  - Real `ClaudeAccounts` (`/api/accounts/*` replacing `post_accounts_any`), `CodexAccounts` (`~/.codex/auth.json` seat swap), and `AntigravityAccounts` (`state.vscdb` sync).

- [ ] **Step 1: Write failing tests for `status-line` chaining, `/api/models` writing a temp `settings.json` + `CLAUDE.md`, accent-insensitive `vault` search (`"toi uu ram"` matching `"Tối ưu RAM"`), and account seat swap**
- [ ] **Step 2: Run `cargo test -p station-core` to verify FAIL**
- [ ] **Step 3: Implement `StatusLine`, `/api/models`, `VaultStore` scanner + `fold()`, and account managers; remove `post_accounts_any`, `get_generic_ok`, and `post_local_ok` completely (`0` stub handlers left!)**
- [ ] **Step 4: Run `cargo test -p station-core` and grep `crates/station-core/src/http` to confirm zero dummy `ok: true` stub fns remain**
- [ ] **Step 5: Commit** (`feat(core): add status-line hook, Claude model config, transcript vault, and account switchers`)

---

### Task 5: RFC 8291/8292 P-256 Web Push, `cloudflared` Tunnel Listener & Interactive `/phone` PWA

**Files:**
- Modify: `Cargo.toml`, `crates/station-core/Cargo.toml`
- Modify: `crates/station-core/src/store/push.rs`
- Create: `crates/station-core/src/store/tunnel.rs`
- Modify: `crates/station-core/src/http/phone.rs`

**Interfaces:**
- Produces:
  - `PushStore`: generates real P-256 keypair (`p256::SecretKey`), stores `{ publicKey, privateKey }` in `data/vapid.json`, signs ES256 VAPID JWT, encrypts `aes128gcm` payloads, and sends push notifications when `hold_plan` selects `by == "phone"`.
  - `TunnelManager`: spawns `cloudflared tunnel` (`CREATE_NO_WINDOW`) pointing to an ephemeral `127.0.0.1:0` Axum listener carrying `.layer(Extension(ViaTunnel(true)))`, supporting quick tunnels and `/api/tunnel/domain`.
  - `phone.rs`: serves `public/sw.js` push handler and interactive mobile UI (live SSE sessions, Allow/Deny approval cards, group chat, prompt send, push subscribe).

- [ ] **Step 1: Write failing tests verifying: (a) generated `publicKey` decodes to a valid 65-byte uncompressed point on NIST P-256 (`p256::PublicKey::from_sec1_bytes`), (b) `aes128gcm` encryption produces non-empty ciphertext, and (c) dedicated tunnel listener enforces PIN auth even from `127.0.0.1` (Review Focus #4)**
- [ ] **Step 2: Run `cargo test -p station-core push` to verify FAIL**
- [ ] **Step 3: Implement P-256 VAPID + RFC 8291 encryption in `push.rs`, `TunnelManager` in `tunnel.rs`, and full `/phone` + `/sw.js` in `phone.rs`**
- [ ] **Step 4: Run `cargo test -p station-core` to verify PASS**
- [ ] **Step 5: Commit** (`feat(core): implement real P-256 Web Push, dedicated ViaTunnel listener, and phone PWA`)

---

### Task 6: Live `OfficeState ↔ UiState ↔ 3D` Sync, All 12 `UiAction`s, `bevy_picking` & WinMM Audio

**Files:**
- Modify: `Cargo.toml` (add `"Win32_Media_Audio"` to `windows-sys`)
- Modify: `crates/station-3d/src/{audio.rs,people.rs,desks.rs,interact.rs,lib.rs}`
- Modify: `crates/station-ui/src/state.rs`
- Modify: `crates/station-app/src/{main.rs,bridge.rs}`

**Interfaces:**
- Produces:
  - `UiState::new_live(...)` (starts with `0` fake sessions/perms — `with_demo_seed` used ONLY when `demo_running` or `--capture-scene` demo preset is active!).
  - Unconditional `sync_live_state_to_ui` & `sync_live_sessions_to_3d` (clears empty lists cleanly; aligns `"work"`/`"wait"`/`"rest"`; syncs `sys`, `burn`, `chat`, `vault`, `pairing_pin`, `clock_label`; spawns/updates/despawns 3D `ChibiWorker`s at assigned `DeskSlot`s and swaps `ScreenMaterials` by active tool).
  - Real `play_sfx(kind: SfxKind)` in `audio.rs` calling Win32 `PlaySoundW(wav_ptr, 0, SND_MEMORY | SND_ASYNC | SND_NODEFAULT)` on pre-baked WAV buffers when `SfxEngine.enabled` is true.
  - Real `bevy_picking` pointer observers (`Pointer<Over>`, `Pointer<Out>`, `Pointer<Click>`) moving `HoverRing` and opening `PlaceCardState` / `ActiveDrawer::Session`.
  - Complete handlers for all remaining `UiAction`s (`SetWeatherCity`, `SetSound`, `ToggleCameraFp`, `TriggerPhoto` PNG readback to `data/shots/`, `ToggleDemo`, `ToggleReplay`, `SendPrompt`, `PostGroupChat`, `SaveWhiteboard` → `/api/todos` + 3D whiteboard redraw, `GpuPrefUi` persisted in `prefs.json`).

- [ ] **Step 1: Write failing tests in `station-ui`, `station-3d`, and `station-app` proving: (a) live `UiState::new_live` has `0` sessions and `working_count()` counts `"work"` sessions accurately, (b) clearing the last session/perm in `OfficeState` clears `UiState` & despawns dynamic 3D workers, and (c) `SfxEngine` pre-bakes all 27 valid RIFF WAV buffers (Review Focus #3)**
- [ ] **Step 2: Run `cargo test -p station-ui` and `cargo test -p station-app` to verify FAIL**
- [ ] **Step 3: Implement live sync, 12 `UiAction` handlers, `bevy_picking` observers, and WinMM `PlaySoundW` output**
- [ ] **Step 4: Run `cargo test --workspace` to verify PASS**
- [ ] **Step 5: Commit** (`feat(app,3d): wire live session 3D sync, all 12 UiActions, bevy_picking, and WinMM SFX`)

---

### Task 7: Dual-Server Side-by-Side Golden Oracle Contract Certification (`run-oracle-contract-tests.mjs`)

**Files:**
- Modify: `scripts/run-oracle-contract-tests.mjs`
- Modify: `crates/station-launcher/src/detect_win32.rs` (make unit test temp-dir portable instead of failing on machines without `ZCode.exe`)
- Modify: `PARITY_LEDGER.md`

- [ ] **Step 1: Upgrade `scripts/run-oracle-contract-tests.mjs` to boot BOTH `node d:/mystation/my-station/server/index.mjs` (`OFFICE_NO_NEXT=1`) AND `agents-station.exe serve` on ephemeral ports, send identical hook/OTLP/chat/pin/undo/vault/perm requests (including `{ id, op: "add", text }` on `/api/pins`, `{ op: "say", text }` on `/api/chat`, `?id=` on `/api/undo`, cross-port `Origin: http://localhost:3000` `403`, `?day=../../x` `400`, post-kill `vault` & `burn` survival, and blocking `/hook/permission` round-trip), and assert matching HTTP status codes & JSON schemas (Review Focus #5)**
- [ ] **Step 2: Run `cargo test --workspace` and `node scripts/run-oracle-contract-tests.mjs` (70s soak: `≤ 35 MB`, `0` PowerShell)**
- [ ] **Step 3: Mark verified P1 backend/CLI/launcher rows `[x]` in `PARITY_LEDGER.md` and commit** (`test(r2): certify dual-server Oracle contract parity and complete Stage 1 remediation`)
