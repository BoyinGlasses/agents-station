# Agents Station — Plan 4: `station-3d` People, Backdrops, Life, Demo, Replay & Audio

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task under a single Cargo compiler lock. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the 3D living world in `crates/station-3d` across all 88 P4 Oracle files: Chibi workers (`Toy`/`Pixar` looks) + office NPC routines, all 3 swappable backdrops (`City` with Vietnamese shops/ATM/EV/photo studio/pickleball/traffic, `Countryside`, `Beach`) with **True Unload**, all 15 office `life/` systems, `demo/` + `replay/` engines, and all 28 procedural `audio/sfx` tones.

**Architecture:** All chibi parts and backdrop buildings reuse `WorldPalette` shared unit meshes and deduplicated materials. Backdrops tag every entity with `BackdropEntity` and track their GPU image handles separately so switching `City ↔ Countryside ↔ Beach` despawns 100% of inactive geometry and purges inactive GPU textures (`≤ 15 MB` drift across 3 cycles). Procedural audio synthesizes 16-bit PCM WAV buffers in memory with exact Oracle frequencies, envelopes, and per-sound cooldowns.

**Tech Stack:** Bevy 0.16, `station-canvas2d`, `station-core`.

**Spec:** `docs/superpowers/specs/2026-11-25-agents-station-design.md`

## Global Constraints

- Minimal Bevy 0.16 features unchanged; **Single Compiler Lock**; `D:\mystation\cargo-target`.
- **True Backdrop Unload:** Switching backdrops despawns every `BackdropEntity` and purges backdrop GPU images immediately (`≤ 15 MB` drift across 3 full cycles).
- **Hard RAM Budgets:** `3D Pro + City + Chibis ≤ 300 MB` Private Working Set; `3D HQ (44 desks) ≤ 380 MB`.
- **Parity Ledger:** Mark all 88 P4 files `[x]` in `PARITY_LEDGER.md` upon Gate P4 completion.

## Review Focus

1. **Chibi rig visibility & zero floating/clipping:** Both `Toy` and `Pixar` looks sit cleanly on office chairs at exact desk coordinates, face their monitors, and animate typing/bobbing/walking smoothly.
2. **True Backdrop Unload isolation:** Unloading `City` removes 0 office interior `WorldEntity` objects while removing 100% of `BackdropEntity` objects and their canvas2d shop/ATM/EV/pickleball GPU textures.
3. **All 28 SFX waveforms & rate-limiting:** Synthesizing any of the 28 Oracle sound names produces valid non-silent WAV PCM samples and respects per-sound cooldown `GAP` and `muted` state.
4. **Demo & Replay determinism:** Demo mode spawns all 5 agents (`claude`, `codex`, `opencode`, `zcode`, `antigravity`) cleanly; replay scrubber handles empty or out-of-range timestamps without panicking.
5. **Visual Proof across all 3 Backdrops:** `artifacts/g4-{city,country,beach}.png` show chibi workers inside Pro office surrounded by City shops/streets, Countryside terraced paddies/hills, and Beach ocean/palms, inspected directly via `read_file`.

---

### Task 1: Chibi Character Rig (`Toy`/`Pixar`), Workers, Navigation & Office NPC Routines

**Files:**
- Create: `crates/station-3d/src/{chibi.rs,people.rs}`
- Modify: `crates/station-3d/src/lib.rs`

- [ ] **Step 1: Implement `chibi.rs` (`ChibiLook`, `ChibiConfig`, `ChibiWorker`, `spawn_chibi`) and `people.rs` (navigation graph, worker desk sync, NPC routines, `animate_chibis_system`) + unit tests**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

---

### Task 2: True-Unload Backdrops (`City`, `Countryside`, `Beach`)

**Files:**
- Create: `crates/station-3d/src/backdrops.rs`
- Modify: `crates/station-3d/src/lib.rs`

- [ ] **Step 1: Implement `BackdropKind`, `BackdropEntity`, `BackdropAssets`, `spawn_backdrop`, `unload_backdrop`, and `animate_backdrop_traffic_system` covering City (shops/ATM/EV/photo/pickleball/signals/traffic), Countryside, and Beach + true-unload unit test**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

---

### Task 3: Office Life Systems (Altar, Weather/Rain, Seasons, Nap, Drill, Incidents, Trophies, Photostrips)

**Files:**
- Create: `crates/station-3d/src/life.rs`
- Modify: `crates/station-3d/src/lib.rs`

- [ ] **Step 1: Implement `OfficeLifeState`, `spawn_life_elements` (lobby Ông Địa altar, seasonal décor, trophy shelf, photostrip strip pins, rain streaks), `update_life_system` + unit tests**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

---

### Task 4: Demo Mode, Replay Scrubber & 28-Sound Procedural Audio Synthesizer

**Files:**
- Create: `crates/station-3d/src/{demo.rs,replay.rs,audio.rs}`
- Modify: `crates/station-3d/src/lib.rs`

- [ ] **Step 1: Implement `demo.rs`, `replay.rs`, and `audio.rs` (all 28 Oracle SFX including Mixue "Oh! Susanna" melody, rate-limit `GAP`, WAV PCM synthesizer) + unit tests**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

---

### Task 5: Wire Full Living World & Certify Gate P4 Visuals (`City`, `Countryside`, `Beach`) & RAM

**Files:**
- Modify: `crates/station-3d/src/lib.rs`, `crates/station-app/src/main.rs`, `PARITY_LEDGER.md`

- [ ] **Step 1: Wire `BackdropKind`, chibi workers, life elements, and demo cast into `OfficeWorldPlugin` + `--capture-scene {pro-city,pro-country,pro-beach}`**
- [ ] **Step 2: Capture `artifacts/g4-{city,country,beach}.png`, visually inspect all 3 via `read_file`, measure Windows Private RAM (`≤ 300 MB`)**
- [ ] **Step 3: Mark all 88 P4 files `[x]` in `PARITY_LEDGER.md`, run `cargo test --workspace`, commit, and push Gate P4 to `origin/main`**
