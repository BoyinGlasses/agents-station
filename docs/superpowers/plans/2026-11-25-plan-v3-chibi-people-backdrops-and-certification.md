# Agents Station — Plan V3: 1:1 Sculpted Indexed `ChibiRig`, 26 NPC Routines, 15 `life/` Systems, Full Vietnamese Street/Backdrops & Final Certification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete 100% 1:1 visual & behavioral parity by porting the sculpted mathematical `ChibiRig` (6 hairstyles, layered Pixar eyes, 3 outfits, 2-bone arm IK, 15+ lazy props in `< 2 MB` shared VRAM), walk-graph navigation + subagent chibis, all 26 NPC cultural routines, all 15 `life/` systems (*Bàn thờ Thần Tài*, *PCCC* fire drill, 4 seasons, rain/weather), and the complete exterior `City` (10 walk-in Vietnamese shops + 19 street modules + `cars.json`), `Countryside`, and `Beach` backdrops, followed by final E2E visual & RAM certification.

**Architecture:** Precompute shared indexed `Handle<Mesh>` assets once in `ChibiMeshCache` (`hair_shell` at `NI=48, NJ=16` indexed = ~4.6 KB/hairstyle vs 1.64 MB unindexed in JS; Lathe torso; swept ponytails/brows; lazy-spawned props). Drive workers, subagents, and NPCs over the exact `NODES`/`ADJ` navigation graph. Build active backdrop modules modularly under `crates/station-3d/src/street/` so switching `City ↔ Countryside ↔ Beach` despawns and purges 100% of inactive backdrop meshes, materials, and `RENDER_WORLD` textures (`≤ 15 MB` drift).

**Tech Stack:** Rust 1.97+, `bevy` `0.16`, `station-canvas2d`, `station-core`, `station-ui`.

**Spec:** `d:\mystation\agents-station\docs\superpowers\specs\2026-11-25-remediation-and-1to1-visual-spec.md`

## Global Constraints

- Prerequisite: Plan V2 (`2026-11-25-plan-v2-render-rooms-and-props-1to1.md`) completed and green.
- Single Compiler Lock (`target-dir = "D:/mystation/cargo-target"`, `jobs = 2`).
- Total `ChibiMeshCache` geometry memory across ALL characters (even 44 HQ workers + subagents + street crowd) MUST stay `< 2 MB` via shared indexed `Handle<Mesh>` and lazy prop attachment (never pre-spawning 15 hidden props per character!).
- Multi-panel `Canvas2d` atlases (`atm`, `ev`, `photo`, `pickle`, `shops`) MUST use proper sub-rect UVs (`uv_rect_quad(u0, v0, u1, v1)`) — NEVER paste an uncropped sprite sheet onto a single quad.
- Final Release Certification Gate MUST prove: `serve ≤ 35 MB`, `Tray ≤ 45 MB`, `2D Lite ≤ 110 MB`, `3D Pro ≤ 300 MB`, `3D HQ ≤ 380 MB`, `3× backdrop drift ≤ 15 MB`, `0` PowerShell spawns, and `571/571` genuinely verified `[x]` in `PARITY_LEDGER.md`.

## Review Focus

1. **Chibi geometry blowing up RAM (like Three.js's 80 MB) or looking bald/cylindrical** — `hair_shell(NI=48, NJ=16)` must generate indexed smooth-normal `Mesh`es cached once in `ChibiMeshCache` (`6` styles × `< 30 KB` = `< 200 KB` total!) and attach held props (`mug`, `noodle`, `paddle`, etc.) only while active, despawning when stowed; pinned by unit test in Task 1.
2. **Workers walking straight through glass walls or subagents failing to link to parent session** — `NavGraph::shortest_path` over `NODES`/`ADJ` must route through doorway nodes without crossing wall segments, and `0.78x` subagent chibis must walk from lift → parent desk with dashed `team-lines`; pinned by unit test in Task 2.
3. **Uncropped multi-panel atlas textures on street props (`ATM`, `V-Green`, `Photobooth`, `Pickleball`, `Shops`)** — `uv_rect_quad` must map `[u0..u1, v0..v1]` cleanly to fascia, screen, and side faces; pinned by unit test & close-up PNG check in Task 5.
4. **First-person walk-in (`Walk in` / `F`) clipping through walls or unable to ride the lift down to the street & enter walk-in shops** — `FirstPersonController` must enforce the 10 cm obstacle grid, support lift transition (`Office ↔ Street`), and trigger `E` shop/billiards/console interactions; pinned in Task 6.
5. **Backdrop memory leak when cycling `City → Countryside → Beach → City` with full shops & street life** — every mesh, material, and texture created for a backdrop must register in `BackdropAssets` (never global `WorldPalette`) and release cleanly on switch (`≤ 15 MB` Private WS drift over 3 cycles); pinned in Task 7.

---

## File Structure

- Modify: `crates/station-3d/src/chibi.rs` + Create: `crates/station-3d/src/chibi_meshes.rs`, `crates/station-3d/src/chibi_props.rs` — port of `chibi-rig.js` (`hair_shell`, `sweep`, `lathe_profile`, 6 hairstyles, layered eyes/lashes/brows/mouth, `Toy` & `Pixar` dims, `hoodie`/`suit`/`shirt` outfits, 2-bone arm IK, radial drop shadow, coats/hats/lanyards, 15+ lazy props).
- Create: `crates/station-3d/src/nav.rs`, Modify: `crates/station-3d/src/people.rs` + Create: `crates/station-3d/src/cast/{mod.rs,staff_roles.rs,office_games.rs,meals_breaks.rs,street_outings.rs}` — port of `navigation.js`, `workers.js` (live desk/station walks + `0.78x` subagents + `team-lines.js`), and all 26 `src/office/people/*.js` NPC & cultural modules.
- Modify: `crates/station-3d/src/life.rs` + Create: `crates/station-3d/src/life_systems/{mod.rs,altar.rs,drill_lift.rs,seasons_weather.rs,workflow_life.rs}` — port of all 15 `src/office/life/*.js` modules (*Bàn thờ Thần Tài*, *Diễn tập PCCC*, lift maintenance, Halloween/Christmas/Tết, Open-Meteo rain + wet-floor cone, postbox, incidents, quota, server test board, persisted trophies & photostrips) + live `demo.rs` simulation loop & `replay.rs` scrubber.
- Create: `crates/station-3d/src/street/{mod.rs,city_grid.rs,shops_walkin.rs,eat_com.rs,traffic.rs,scenes_a.rs,scenes_b.rs,country_beach.rs}` + Modify: `crates/station-canvas2d/src/street_surfaces.rs`, `crates/station-3d/src/backdrops.rs` — port of all 36 `src/office/world/backdrops/*.js` files with `uv_rect_quad` mapping and full `BackdropAssets` purge.
- Modify: `crates/station-3d/src/{camera.rs,interact.rs}` — 10 cm obstacle grid collision, lift-to-street first-person transition, in-shop/pool/TV-game `E` interactions.

---

### Task 1: 1:1 Sculpted Indexed `ChibiRig` (`hair_shell`, `sweep`, `lathe`, Layered Eyes, 3 Outfits, 2-Bone IK & Lazy Props `< 2 MB`)

**Files:**
- Create: `crates/station-3d/src/{chibi_meshes.rs,chibi_props.rs}`
- Modify: `crates/station-3d/src/chibi.rs`

**Interfaces:**
- Consumes: Exact math from `d:\mystation\my-station\src\office\people\chibi-rig.js`.
- Produces:
  - `#[derive(Resource)] pub struct ChibiMeshCache` holding shared `Handle<Mesh>` for `Toy` & `Pixar` Lathe torsos, 6 sculpted hairstyles (`Bob`, `Plain`, `Spiky`, `Ponytail`, `Bun`, `Curly` via `hair_shell(r, line, opts, 48, 16)` + `sweep`), eye sclera/iris/pupil/highlight/lash/brow, 3 outfits (`Hoodie`, `Suit`, `Shirt`), limbs, shoes, radial shadow quad, and 15+ prop meshes.
  - `pub fn solve_two_bone_ik(shoulder: Vec3, target: Vec3, pole: Vec3, upper_len: f32, fore_len: f32) -> (Quat, Quat)`
  - `pub fn set_active_prop(commands: &mut Commands, cache: &ChibiMeshCache, chibi: Entity, prop: Option<PropKind>)` (spawns only the 1 active prop, despawns when `None`).

- [ ] **Step 1: Write failing unit tests in `chibi_meshes.rs` proving: (a) all 6 `hair_shell` meshes have valid indexed triangles, smooth normals, and total CPU/GPU vertex byte size across the entire `ChibiMeshCache` is `< 1_500_000` bytes (`< 1.5 MB`), and (b) `solve_two_bone_ik` places the hand within `0.01m` of keyboard target (Review Focus #1)**
- [ ] **Step 2: Run `cargo test -p station-3d chibi` to verify FAIL**
- [ ] **Step 3: Implement `chibi_meshes.rs`, `chibi_props.rs`, and hierarchical rigged `chibi.rs` (`Toy` & `Pixar` switchable)**
- [ ] **Step 4: Capture close-up `--capture-scene chibi-closeup` PNG and visually compare hair locks, eyes, hoodie/suit/shirt, and typing pose against `my-station` via `read_file`**
- [ ] **Step 5: Commit** (`feat(3d): port 1:1 sculpted indexed ChibiRig, 6 hairstyles, 3 outfits, 2-bone IK, and lazy props`)

---

### Task 2: Room-Door Navigation Graph (`NODES`/`ADJ`), Live Worker Walks, Subagents (`0.78x` + `team-lines`) & Office Cast (`director`, `hr`, `accountant`, `staff`, `guard`, `interns`, `farewell`)

**Files:**
- Create: `crates/station-3d/src/nav.rs`, `crates/station-3d/src/cast/{mod.rs,staff_roles.rs}`
- Modify: `crates/station-3d/src/people.rs`

**Interfaces:**
- Consumes: `d:\mystation\my-station\src\office\people\{navigation,workers,team-lines,director,rounds,hr,accountant,staff,guard,interns,farewell}.js`.
- Produces:
  - `NavGraph::for_layout(layout: OfficeLayout)` + `shortest_path(from: Vec3, to: Vec3) -> Vec<Vec3>`
  - Live session worker lifecycle (lift arrival → walk to assigned desk → tool walks to Library / Globe / Server Racks / Whiteboard / Coffee → resting / sleeping `"z"` sprites → farewell departure) + `0.78x` subagent chibis briefing with parent worker + dashed `team-lines`.
  - Full NPC routines: `Director` (prompt delivery + 10:00/15:30 rounds), `HR` (lift clock-in terminal, newcomer escort, birthday cake), `Accountant` (API burn monitor + 17:30 report), `Receptionist` / `Cleaner` / `Shopee` courier, street `Guard`, `Interns` (orange lanyards + 20-turn permanent contract stamp), and `Farewell` packing/unpacking.

- [ ] **Step 1: Write failing unit tests in `nav.rs` proving paths from Director room / Lobby / Wing to Desks route strictly through doorway nodes without crossing partition wall segments (Review Focus #2)**
- [ ] **Step 2: Run `cargo test -p station-3d nav` to verify FAIL**
- [ ] **Step 3: Implement `nav.rs`, upgraded `people.rs` (workers + subagents + `team-lines`), and `cast/staff_roles.rs`**
- [ ] **Step 4: Run `cargo test -p station-3d` to verify PASS**
- [ ] **Step 5: Commit** (`feat(3d): add doorway nav-graph, subagent chibis, and full office staff NPC routines`)

---

### Task 3: Cultural & Leisure Routines (`billiards`, `foosball`, `gaming`, `karaoke`, `comnha`, `comtrua`, `breaks`, `teatime`, `outings`, `nhau`, `payday`, `photobooth`, `pickle`, `evswap`, `duty`, `errands`, `gatherings`)

**Files:**
- Create: `crates/station-3d/src/cast/{office_games.rs,meals_breaks.rs,street_outings.rs}`
- Modify: `crates/station-3d/src/cast/mod.rs`

**Interfaces:**
- Consumes: Remaining 17 files in `d:\mystation\my-station\src\office\people\*.js`.
- Produces: Scheduled & event-driven worker activities: 2-player pool & foosball matches, sofa console gaming, tone-melody `karaoke` (`karaoke-songs.js` original track list + scoring), noon microwave lunch queue (`comnha`) & street *cơm bình dân / bún đậu* (`comtrua`), afternoon coffee & Friday bubble-tea (`breaks`, `teatime`), convenience-store `outings`, evening *quán nhậu* (`"1, 2, 3 dô!"` + Grab home), payday ATM queue, release Korean `photobooth` flash, green-CI `pickleball`, `<20%` `evswap`, *trực nhật* `duty` chores, prop `errands`, and bell `gatherings`.

- [ ] **Step 1: Write failing unit tests verifying deterministic schedule triggers (noon `12:00` lunch, `16:00` Friday teatime, release tag → photobooth, green CI after hours → pickleball)**
- [ ] **Step 2: Run `cargo test -p station-3d cast` to verify FAIL**
- [ ] **Step 3: Implement `office_games.rs`, `meals_breaks.rs`, and `street_outings.rs`**
- [ ] **Step 4: Run `cargo test -p station-3d` to verify PASS**
- [ ] **Step 5: Commit** (`feat(3d): port all 17 office leisure, Vietnamese lunch, karaoke, and street outing routines`)

---

### Task 4: All 15 `life/` Systems (*Bàn thờ Thần Tài*, *Diễn tập PCCC*, Lift Service, 4 Seasons, Live Rain/Weather, Postbox, Incidents, Quota, Tests Board) + Live `Demo` & `Replay`

**Files:**
- Create: `crates/station-3d/src/life_systems/{mod.rs,altar.rs,drill_lift.rs,seasons_weather.rs,workflow_life.rs}`
- Modify: `crates/station-3d/src/{life.rs,demo.rs,replay.rs}`

**Interfaces:**
- Consumes: All 15 `d:\mystation\my-station\src\office\life\*.js` files + `demo/{demo,demo-log}.js` + `replay/{replay,timeline}.js`.
- Produces:
  - Detailed red-and-gold *Bàn thờ Thần Tài – Thổ Địa* (upturned 2-tier roof, couplets, Ông Địa & Thần Tài, gold toad, 3 cups, incense bowl + Mùng 1/Rằm fruit tray & smoke), quarterly *Diễn tập PCCC* fire drill (strobes, stair evacuation, roll call `"Có!"`, extinguisher demo), post-update lift maintenance technician, date-driven `Halloween` / `Christmas` / `Tet` / `Normal` decorations, background Open-Meteo poller + spawned `RainDrop`s + `"SÀN ƯỚT"` cone & umbrella stand, commit `Postbox` + postman on `git push`, `Incidents` red strobes, `Quota` break sign, server-room `Tests` lamp board, live `Trophies` shelf (`data/totals.json`) & up to 6 `Photostrips` (`data/shots/`), plus live `Demo` simulation emitter & `Replay` timeline world-state seeker.

- [ ] **Step 1: Write failing unit tests for `SeasonTheme::from_date` (Oct 31 → `Halloween`, Dec 24 → `Christmas`, Lunar Tết → `Tet`), `Demo` live event tick, and `Replay` state seek**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement `life_systems/*` and wire live `demo.rs` & `replay.rs` into `OfficeWorldPlugin` & `UiAction::{ToggleDemo, ToggleReplay}`**
- [ ] **Step 4: Run `cargo test -p station-3d` to verify PASS**
- [ ] **Step 5: Commit** (`feat(3d): port Than Tai altar, fire drill, 4 seasons, rain/weather, workflow life, demo, and replay`)

---

### Task 5: 1:1 `City` Supergrid, UV-Mapped Street Props, All 10 Walk-In Vietnamese Shops, `eat-com` & All 19 Street Modules

**Files:**
- Create: `crates/station-3d/src/street/{mod.rs,city_grid.rs,shops_walkin.rs,eat_com.rs,traffic.rs,scenes_a.rs,scenes_b.rs}`
- Modify: `crates/station-canvas2d/src/street_surfaces.rs`, `crates/station-3d/src/{palette.rs,backdrops.rs}`

**Interfaces:**
- Consumes: `d:\mystation\my-station\src\office\world\backdrops\{city,shops,shop-*,eat-com,street-*}.js` + `assets/vehicles/cars.json`.
- Produces:
  - `WorldPalette::uv_rect_quad(meshes, u0, v0, u1, v1) -> Handle<Mesh>` (fixes the uncropped atlas bug!).
  - `city_grid.rs`: multi-block street supergrid, tiled sidewalks, crosswalks, night-lit building window atlases, stepped skyscrapers, rooftop HVAC/water-tanks/beacons, streetlamps with night light pools, utility poles & sagging wires (`street-wires`), live phase-counting traffic signals (`street-signals`).
  - `shops_walkin.rs` + `eat_com.rs`: 8–10 city-selectable **walk-in 3D ground-floor shops** (`Cộng` peacock cushions & vintage TV, `Highlands`, `Phúc Long`, `Mixue` + dancing Snow King, `TGDĐ` + inflatable tube dancer, `ĐMX` TV wall & massage chairs, `Katinat`, `Long Châu`, `GS25`, `Circle K`, `WinMart+`/`BHX` night shutters) + *Quán cơm bình dân & Friday Bún đậu mắm tôm* (`eat-com` 8 dish trays, inox tables, plastic stools).
  - `traffic.rs` + `scenes_a.rs` + `scenes_b.rs`: 13 articulated vehicle types from `cars.json` (VinFast VF3/VF8/XanhSM, Grab scooters, buses stopping at stops, 115/114 sirens `street-siren`, sunny `"ninja Lead"` & bulky loads `street-riders`, ≥33°C shade-stopping `street-shade`, zebra pedestrians `street-folk`), *CSGT* checkpoint (`street-csgt`), morning *xôi/bánh cuốn* (`street-morning`), *xe ôm*/hammock/barber/tire-repair (`street-corner`, `street-repair`), lunar votive bins (`street-lunar`), *công viên dưỡng sinh/đá cầu/cờ tướng* (`street-park`), evening *quán nhậu* (`street-nhau`), weekend *rạp cưới* (`street-wedding`), and UV-mapped 3D `street-atm`, `street-ev` (scooter row + laptop battery bars), `street-photo` (curtained booths + flash), and `street-pickle` (20×44ft court + doubles rally).
  - Every entity, mesh, material, and texture registered inside `BackdropAssets` for 100% clean unload!

- [ ] **Step 1: Write failing unit tests proving: (a) `uv_rect_quad(0.0, 0.0, 0.5, 0.5)` binds exact UVs within `[0.0..0.5]`, (b) `traffic.rs` builds meshes for all 13 `cars.json` vehicle specs, and (c) `shops_walkin` spawns walk-in interiors for the active city set (Review Focus #3)**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement `uv_rect_quad` and `street/{city_grid,shops_walkin,eat_com,traffic,scenes_a,scenes_b}.rs`**
- [ ] **Step 4: Capture `--capture-scene pro` (wide + street close-up + shop interior) and visually compare side-by-side against `my-station` street/shop captures via `read_file`**
- [ ] **Step 5: Commit** (`feat(3d): port 1:1 City supergrid, 10 walk-in VN shops, eat-com, and all 19 street life scenes`)

---

### Task 6: 1:1 `Countryside` & `Beach` Backdrops + First-Person Obstacle Grid, Lift-to-Street & Shop/Pool Interactions

**Files:**
- Create: `crates/station-3d/src/street/country_beach.rs`
- Modify: `crates/station-3d/src/{backdrops.rs,camera.rs,interact.rs}`

**Interfaces:**
- Consumes: `d:\mystation\my-station\src\office\world\backdrops\{countryside,beach}.js` and `interact/first-person.js`.
- Produces:
  - `Countryside`: meandering river + wooden bridge + *cổng làng* village gate + bamboo/banana/areca + grazing water buffalo + flapping egrets + kids flying kites + 260 night fireflies.
  - `Beach`: vertex-colored wet/dry shoreline, animated surf foam waves, 70 palms, loungers, *thuyền thúng* basket boats, bobbing fishing boats with red flags, string-lit thatched huts, rotating night lighthouse beam, flapping seagulls.
  - `FirstPersonController`: 10 cm obstacle grid collision, visible `"Bạn"` 3rd-person avatar during transitions, elevator ride `Office ↔ Street` with `fp-fade`, and `E` near-prompts (browse/buy in walk-in shops, play pool shot with Spacebar power bar `bil-hud`, sit on sofa to play `tv-games`).

- [ ] **Step 1: Write failing unit tests for 10 cm obstacle grid wall blocking and `BackdropAssets` full cleanup across `City → Countryside → Beach → City` (`0` leaked images/materials/meshes) (Review Focus #4 & #5)**
- [ ] **Step 2: Run `cargo test -p station-3d` to verify FAIL**
- [ ] **Step 3: Implement `country_beach.rs` and `FirstPersonController` in `camera.rs` / `interact.rs`**
- [ ] **Step 4: Capture `--capture-scene pro-country` and `--capture-scene pro-beach` and visually verify via `read_file`**
- [ ] **Step 5: Commit** (`feat(3d): port 1:1 Countryside and Beach backdrops and first-person street/shop walk-in`)

---

### Task 7: Final Side-by-Side Visual Diff, RAM/Leak Soak & `571/571` Honest Release Certification

**Files:**
- Modify: `docs/certification/2026-11-25-release-certification.md`
- Modify: `PARITY_LEDGER.md`

- [ ] **Step 1: Build `cargo build --release` (single compiler lock)**
- [ ] **Step 2: Capture matching camera presets from BOTH Golden Oracle (`d:\mystation\my-station` via Playwright) and `agents-station.exe --release` (`cozy`, `pro`, `hq`, `pro-night`, `pro-country`, `pro-beach`, `street-shops`, `chibi-closeup`, `hud-pro-vi`, `lite-2d-vi`) and inspect all 10 side-by-side pairs via `read_file`, fixing any remaining visual discrepancy**
- [ ] **Step 3: Run full `cargo test --workspace` + dual-server `node scripts/run-oracle-contract-tests.mjs` + live Windows Private WS soak across all modes (`serve ≤ 35 MB`, `Tray ≤ 45 MB`, `2D Lite ≤ 110 MB`, `3D Pro ≤ 300 MB`, `3D HQ ≤ 380 MB`, `3× backdrop cycle drift ≤ 15 MB`, `3D → 2D → Tray → 3D` round-trip clean, `0` `powershell.exe` children)**
- [ ] **Step 4: Verify `571/571` items in `PARITY_LEDGER.md` are genuinely `[x]`, update `docs/certification/2026-11-25-release-certification.md` with honest numbers & side-by-side proof, commit, and push `origin/main`** (`docs(cert): certify 1:1 visual, behavioral, security, and low-RAM release parity`)
