# Agents Station — Plan V2: 1:1 Render Pipeline, `cosmic-text` Surfaces, Pro/HQ/Cozy Rooms & 22 Animated Props Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade `station-canvas2d` and `station-3d` so the interior office (`Cozy`, `Pro`, `HQ`), lighting/post-processing, procedural floor/wall/window textures, furniture, and all 22 interactive/animated props (including the 3 playable pixel-art `tv-games`) are visually indistinguishable from `my-station`'s `layout-pro.jpg` while staying well under the `300 MB` RAM budget.

**Architecture:** Match `renderer.js` & `lights.js` with `Msaa::Sample4`, `Tonemapping::AcesFitted` (`1.12×` exposure), procedural in-memory Studio Day/Night cubemap `EnvironmentMapLight` (`< 1.5 MB`), dual sky/ground-bounce directional fill (`#FFF4DC` / `#A9B98A`), `2048` tight ortho shadows, and night `Bloom`. Replace `ab_glyph` with one shared `cosmic-text` shaper, generate crisp tileable `RENDER_WORLD` floor/wall/window textures, and port 100% of `pro/floor.js`, `pro/wing.js`, `furniture/*.js`, and `props/*.js` using shared `Handle<Mesh>` / `Handle<StandardMaterial>` caches.

**Tech Stack:** Rust 1.97+, `bevy` `0.16` (`ktx2`/`bevy_core_pipeline` `Bloom`, `EnvironmentMapLight`, `CascadeShadowConfigBuilder`), `cosmic-text` `0.14`, `tiny-skia` `0.11`, `resvg` `0.45`.

**Spec:** `d:\mystation\agents-station\docs\superpowers\specs\2026-11-25-remediation-and-1to1-visual-spec.md`

## Global Constraints

- Prerequisite: Plan V1 (`2026-11-25-plan-v1-ui-shell-i18n-and-drawers-1to1.md`) completed and green.
- Single Compiler Lock (`target-dir = "D:/mystation/cargo-target"`, `jobs = 2`).
- Every static procedural texture MUST use `RenderAssetUsages::RENDER_WORLD` so CPU pixel buffers are freed immediately after GPU upload; dynamic textures (TV games, arcade, clock/LEDs) reuse a single preallocated scratch buffer throttled cleanly.
- Every task MUST read the exact coordinates/colors/dimensions from the named `d:\mystation\my-station\src\office\world\*.js` file before coding — NEVER invent substitute boxes or approximate room layouts from memory.
- Visually compare exported `--capture-scene` PNGs side-by-side against `d:\mystation\my-station\public\assets\thumbs\layout-{cozy,pro,hq}.jpg` via `read_file` before committing.

## Review Focus

1. **Missing Unicode symbols (`≤`), broken Thai combining tone marks, or clipped Vietnamese diacritics on 2D canvas textures** — `cosmic-text` `FontSystem` + `SwashCache` with bundled `BeVietnamPro` + `JetBrainsMono` + `PlexThaiLooped` + system symbol fallback must shape `"≤ 300 MB · Tiếng Việt · ภาษาไทย"` cleanly with zero missing-glyph boxes; pinned by test in Task 1.
2. **Ambient washout or jagged stair-stepping making Bevy look like flat clay** — camera must enable `Msaa::Sample4`, `Tonemapping::AcesFitted`, `Exposure` (~`1.12×`), Studio `EnvironmentMapLight`, upward `#A9B98A` ground-bounce directional light, and `2048` tight ortho cascade (`±21m` Pro) + live `sync_lighting_system` on `ToggleNight`; pinned in Task 2.
3. **Laptop screens slicing through tilted laptop lids or dynamic `tv-games` leaking 2.6 MB `Image` allocations at 60 FPS** — desk screens must inherit `lid_rot` (`0.22` rad X tilt), and live `tv-games` (`256×144` pixel-art upscaled nearest-neighbor!) must mutate a single `RenderAssetUsages::MAIN_WORLD | RENDER_WORLD` `Image` buffer in-place (`~147 KB` total, zero per-frame allocations!); pinned by tests in Tasks 4 & 6.
4. **Detached prop children (like `vacuum.js` LED staying frozen at origin while the vacuum moves)** — every moving prop (`vacuum`, `cat`, `aquarium` fish, `rocket` + confetti, `lift` doors, `clock` hands, `microwave` plate/door) must use proper parent-child `Transform` hierarchies; pinned in Tasks 4 & 5.
5. **VRAM/RAM spike from high-res procedural floor/wall textures** — tileable surfaces (`plankTex` `256×256`, `subwayTile` `128×128`, `checkTile` `128×128`, `speckleRug` `128×128`, `serverGrid` `128×128`) must use `ImageSampler::repeat` UV tiling instead of giant `2048×2048` bitmaps (`< 1 MB` total VRAM for all room floors!); pinned in Task 3.

---

## File Structure

- Modify: `Cargo.toml`, `crates/station-canvas2d/Cargo.toml`, `crates/station-canvas2d/src/{lib.rs,text.rs,canvas.rs,office_surfaces.rs}` — replace `ab_glyph` with `cosmic-text`, add reusable scratch buffer, add tileable floor/wall/window/plaque/ball generators & 3-game `tv_games.rs` pixel engine.
- Create: `crates/station-canvas2d/src/tv_games.rs` — port of `src/office/world/props/tv-games.js` (`football` "GOAL STARS", `racing` "SAIGON RUSH", `fighting` "ROOFTOP FIGHT" + title cycle).
- Modify: `crates/station-3d/src/{camera.rs,lights.rs,palette.rs}` + Create: `crates/station-3d/src/ibl.rs` — `Msaa::Sample4`, `AcesFitted` + exposure, procedural Studio Day/Night cubemap `EnvironmentMapLight`, `Bloom` (night), hemisphere sky+bounce lights, `2048` shadow cascade, live `sync_lighting_system`.
- Modify: `crates/station-3d/src/{rooms.rs,entrance.rs,desks.rs,furniture.rs}` + Create: `crates/station-3d/src/{pro_floor.rs,hq_wing.rs}` — 100% 1:1 geometry & materials for `cozy-room.js`, `pro/{floor,pieces,wing}.js`, `windows.js` (with roller blinds), `terrace.js`, `entrance.js` (animated lift + stairwell behind fire door), `desks.js` (5-caster reclining chair, coat/backpack, 4-stage git-dirty mess, fixed tilted laptop screen), `furniture/*` (ticking 3-hand wall clock, `SHIP IT` neon, framed art, floor lamps, spinning globe, blinking server racks, rocket launch + 70 confetti).
- Modify: `crates/station-3d/src/props.rs` + Create: `crates/station-3d/src/props_detail/{mod.rs,cat.rs,microwave.rs,shredder_cups.rs,leisure.rs}` — 100% 1:1 ports of all 22 `src/office/world/props/*.js` modules.

---

### Task 1: Replace `ab_glyph` with Shared `cosmic-text` Engine (Thai GSUB/GPOS + Symbols + Zero Duplicate TTFs)

**Files:**
- Modify: `Cargo.toml`, `crates/station-canvas2d/Cargo.toml` (remove `ab_glyph`, add `cosmic-text = "0.14"`)
- Modify: `crates/station-canvas2d/src/{lib.rs,text.rs,office_surfaces.rs}`

**Interfaces:**
- Produces:
  - `pub struct FontEngine` wrapping a single `Arc<Mutex<(cosmic_text::FontSystem, cosmic_text::SwashCache)>>` loaded once with bundled `BeVietnamPro`, `JetBrainsMono`, `PlexThaiLooped`, plus fallback math/symbol faces (no 24× `FontVec` duplication!).
  - Fixed strikethrough vertical offset in `draw_duty_board` (`office_surfaces.rs`).

- [ ] **Step 1: Write failing unit tests in `text.rs` asserting non-empty shaped glyph runs without `.notdef` (`glyph_id == 0`) for `"≤ 300 MB"`, `"Tiếng Việt: Ă Ắ Ặ Đ Ơ Ư Ỹ"`, and Thai combining marks `"ผู้จัดการ · ภาษาไทย"` (Review Focus #1)**
- [ ] **Step 2: Run `cargo test -p station-canvas2d` to verify FAIL**
- [ ] **Step 3: Implement `cosmic-text` `FontEngine` and fix `office_surfaces.rs` strikethrough/glyph alignment**
- [ ] **Step 4: Run `cargo test -p station-canvas2d` and inspect regenerated `g2-canvas2d-sheet.png` via `read_file`**
- [ ] **Step 5: Commit** (`feat(canvas2d): replace ab_glyph with shared cosmic-text OpenType shaper`)

---

### Task 2: 1:1 Render Pipeline (`4x MSAA`, `AcesFitted` `1.12x`, Studio IBL Cubemap, Hemisphere Bounce, `2048` Shadows & Live Night Sync)

**Files:**
- Create: `crates/station-3d/src/ibl.rs`
- Modify: `crates/station-3d/src/{camera.rs,lights.rs,lib.rs}`
- Modify: `crates/station-app/src/main.rs`

**Interfaces:**
- Produces:
  - `pub struct OfficeIbl { pub day_diffuse: Handle<Image>, pub day_specular: Handle<Image>, pub night_diffuse: Handle<Image>, pub night_specular: Handle<Image> }` (6-face `64×64` RGBA16F cubemaps matching `RoomEnvironment` day & warm/cool night box in `renderer.js:83-94`).
  - `MainOfficeCamera` configured with `Msaa::Sample4`, `Tonemapping::AcesFitted`, `Exposure { ev100: 9.54 }`, `EnvironmentMapLight`, and `Bloom` (toggled on at night).
  - Dual-hemisphere setup in `lights.rs` (sky ambient + upward `#A9B98A` bounce directional light + `SunLight` with `DirectionalLightShadowMap { size: 2048 }` and layout-tight `CascadeShadowConfig` + `sync_lighting_system` updating sun, bounce, room lamps, IBL, and bloom on `LightingState` change).

- [ ] **Step 1: Write failing tests in `lights.rs` and `ibl.rs` verifying cubemap face dimensions (`6` stacked faces, `TextureViewDimension::Cube`) and `sync_lighting_system` dimming `SunLight` from `15_500` → `800` lux and boosting `RoomLamp`s when `LightingState.night` flips `true` (Review Focus #2)**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement `ibl.rs`, upgrade `camera.rs` & `lights.rs`, and set `DirectionalLightShadowMap { size: 2048 }` in `main.rs`**
- [ ] **Step 4: Run `cargo test -p station-3d`, capture `--capture-scene pro` & `--capture-scene pro-night`, and inspect shadow crispness, glass reflections, and night lighting via `read_file`**
- [ ] **Step 5: Commit** (`feat(3d): match Three.js 4x MSAA, ACES exposure, Studio IBL, bounce light, and 2048 shadows`)

---

### Task 3: Tileable Floor/Wall/Rug/Window `Canvas2d` Textures & 1:1 `Cozy`, `Pro`, and `HQ` Room Architecture

**Files:**
- Modify: `crates/station-canvas2d/src/office_surfaces.rs`
- Modify: `crates/station-3d/src/{palette.rs,rooms.rs,entrance.rs}`
- Create: `crates/station-3d/src/{pro_floor.rs,hq_wing.rs}`

**Interfaces:**
- Consumes: Exact coordinates & colors from `d:\mystation\my-station\src\office\world\{cozy-room.js,windows.js,terrace.js,entrance.js}` and `world/pro/{floor.js,pieces.js,wing.js}`.
- Produces:
  - Repeating `ImageSampler::repeat` textures (`256×256` oak `plank_tex`, `128×128` pantry `checker_tile` & `subway_tile`, `128×128` `server_grid`, `128×128` `speckle_rug`, wainscoting slats) + `1024×512` day/night window paintings + animated `WindowBlind` entities + room door plaques (`Director`, `Meeting`, `Library`, `WC`) + HQ dynamic `DEPT_SIGNS` & 4 Kanban `sprint_board`s.
  - Complete `pro_floor.rs`: exact partition doorway cutouts + open glass double doors with handles, walnut U-shaped reception counter (monitor, phone, bell, guestbook, lily vase, `My Office` glowing sign) + waiting armchair, 2 restroom stalls (pivoting doors + red/green indicator + sink + mirror), 4 pantry dining tables + 16 colored chairs + 2 water coolers + sink faucet, 2 acoustic glass phone booths, filing cabinet, 2 framed posters, library armchairs + 7-rung wooden ladder, lounge beanbag + foosball table, server precision AC cooler with live CPU/°C screen & fire extinguisher.
  - Complete `hq_wing.rs` (credenzas, typed plants `monstera`/`snake`/`bush`, wing sofas, corridor sign) & `entrance.rs` (sliding elevator doors + LED `▼ 12` floor display + full concrete stairwell behind fire door with 6 steps, handrail, `EXIT` sign).

- [ ] **Step 1: Write failing unit tests asserting `WorldPalette::tiled_mat` sets `ImageAddressMode::Repeat` and `pro_floor` spawns reception, 2 WC stalls, 4 dining tables, 2 phone booths, foosball, and elevator/stairwell entities (Review Focus #5)**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement repeating surface generators in `office_surfaces.rs` and full 1:1 room modules (`rooms.rs`, `pro_floor.rs`, `hq_wing.rs`, `entrance.rs`)**
- [ ] **Step 4: Capture `--capture-scene cozy`, `--capture-scene pro`, and `--capture-scene hq` and visually compare every room corner against `layout-{cozy,pro,hq}.jpg` via `read_file`**
- [ ] **Step 5: Commit** (`feat(3d): port 1:1 tiled floors, Pro reception/WC/pantry/phone-booths, HQ wing, and stairwell`)

---

### Task 4: 1:1 Desks (Tilted Screen Fix, Coats/Backpacks, Nap Bedding, Dirty-Git Mess) & Core Animated Furniture

**Files:**
- Modify: `crates/station-3d/src/{desks.rs,furniture.rs}`

**Interfaces:**
- Consumes: `d:\mystation\my-station\src\office\world\{desks.js,screens.js,room-update.js}` and `world/furniture/{board,coffee,meeting,plants-lamps,rocket,room-pieces,server-racks}.js`.
- Produces:
  - Fixed laptop screen transform (`lid_rot * Quat::from_rotation_y(PI)`) + scrolling live tool-specific screen content (`Edit`, `Bash`, `Read`, `Web`) + 5-caster reclining chair + worker coat/backpack (`sync_kit`) + nap pillow/blanket (`bedding.js`) + 4-stage uncommitted git desk mess (1/6/16/41 dirty files: loose sheets, paper balls, mugs, pizza box) + 7 desk trinkets (`duck`, `cactus`, `frame`, `can`, `robot`, `donut`, `books`) & laptop sticky notes (`trinkets.js`).
  - `SHIP IT` glowing sign, ticking 3-hand analog `WallClock` (`hour_hand`, `min_hand`, `sec_hand`), framed wall art, floor lamps, spinning globe, CPU-scaled server fans & blinking rack LEDs, and `DeployRocket` launch + smoke puffs + 70-piece gravity/spin confetti burst.

- [ ] **Step 1: Write failing unit tests verifying desk screen normal vector matches `lid_rot` (`dot > 0.99`) and `WallClock` hand angles match `15:30:00` (Review Focus #3 & #4)**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement 1:1 `desks.rs` and `furniture.rs` + update systems**
- [ ] **Step 4: Run `cargo test -p station-3d` and verify close-up desk/clock/meeting screenshot via `read_file`**
- [ ] **Step 5: Commit** (`feat(3d): port 1:1 desks, tilted screens, trinkets, git mess, analog clock, and rocket confetti`)

---

### Task 5: Articulated Office Props (`cat`, `microwave`, `shredder`, `cups`, `aquarium`, `vacuum`, `printer`, `dev-servers`)

**Files:**
- Create: `crates/station-3d/src/props_detail/{mod.rs,cat.rs,microwave.rs,shredder_cups.rs}`
- Modify: `crates/station-3d/src/props.rs`

**Interfaces:**
- Consumes: `d:\mystation\my-station\src\office\world\props\{cat,microwave,shredder,cups,bins,aquarium,vacuum,printer,dev-servers,duty-board,neon,string-lights,switches}.js`.
- Produces:
  - 20-part articulated tabby `OfficeCat` (stripes, 2 ears, eyes, 4 walking legs, 6-joint wagging tail, nav wandering, sofa/keyboard sitting, `"meow"` + hearts on click).
  - Dual hollow `MicrowaveOvens` (swinging doors, rotating plates, 5 typed lunchboxes `blue`/`green`/`pink`/`glass`/`inox`, rising steam/smell wisps, taped red rule note, dish soap, live LED timer).
  - Transparent-window `Shredder` + per-desk context-window paper reams/sheets/>85% red flags + per-desk waste `bins` + per-desk terracotta prompt-cache `cups` with rising steam wisps (`cache-clock`) + `Aquarium` (gravel, swaying seaweeds, bubbles, tangent-facing fish, click flake feed) + parented-LED `Vacuum` patrolling walk graph + `Printer` paper eject + live per-owner `DevServer` boxes + sagging seasonal `StringLights` + working wall `Switches`.

- [ ] **Step 1: Write failing unit tests proving `Vacuum` LED is a child entity moving with `Vacuum` and `OfficeCat` spawns `>= 16` articulated child meshes (Review Focus #4)**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement `cat.rs`, `microwave.rs`, `shredder_cups.rs`, and upgraded `props.rs`**
- [ ] **Step 4: Run `cargo test -p station-3d` to verify PASS**
- [ ] **Step 5: Commit** (`feat(3d): port 20-part cat, dual microwaves, shredder, cache cups, aquarium, and live props`)

---

### Task 6: 3 Playable/AI Pixel-Art `tv-games` (`GOAL STARS`, `SAIGON RUSH`, `ROOFTOP FIGHT`), `Arcade`, `Billiards` & `Karaoke` Booth

**Files:**
- Create: `crates/station-canvas2d/src/tv_games.rs`
- Create: `crates/station-3d/src/props_detail/leisure.rs`
- Modify: `crates/station-canvas2d/src/lib.rs`, `crates/station-3d/src/props.rs`

**Interfaces:**
- Consumes: `d:\mystation\my-station\src\office\world\props\{tv-games,game-tv,arcade,billiards,karaoke,tv}.js`.
- Produces:
  - `pub struct TvGamesSim` rendering into a single reusable `256×144` RGBA pixel buffer (`147 KB` in-place mutation, zero heap churn!): cycling title screen + full AI/player-controllable `Football` (3v3 pitch, passes, goals, scoreboard), `Racing` (split-screen pseudo-3D road curves, rival cars, turbo), and `Fighting` (2D rooftop arena, health/super bars, fireballs, blocks, K.O. slow-mo) + dynamic ambilight wall `PointLight` matching screen dominant color!
  - Live animated `Arcade` (`draw_arcade_frame(t, coin)`), 16-numbered-ball 6-pocket `BilliardsTable` + cue rack + chalk, and furnished `KaraokeBooth` (hinged glass door, bench, speakers, 2 mics, headphones, scrolling lyric fill).

- [ ] **Step 1: Write failing unit tests in `tv_games.rs` stepping `Football`, `Racing`, and `Fighting` for `120` ticks each, asserting zero buffer reallocations (`ptr` unchanged) and distinct non-static frames (Review Focus #3)**
- [ ] **Step 2: Run `cargo test -p station-canvas2d tv_games` to verify FAIL**
- [ ] **Step 3: Implement `tv_games.rs` and `leisure.rs` (`GameTv` + ambilight, live `Arcade`, 16-ball `Billiards`, `KaraokeBooth`)**
- [ ] **Step 4: Run `cargo test --workspace`, capture interior shots, and verify side-by-side against `layout-pro.jpg` via `read_file` + confirm 3D Pro Private WS `< 260 MB`**
- [ ] **Step 5: Update `PARITY_LEDGER.md` for all `scene/`, `world/` interior, `furniture/`, and `props/` files and commit** (`feat(3d,canvas2d): port 3 playable tv-games, 16-ball billiards, karaoke booth, and certify V2 interior`)
