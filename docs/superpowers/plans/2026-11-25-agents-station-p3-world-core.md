# Agents Station — Plan 3: `station-3d` World Core, Layouts, Furniture & 22 Props

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task under a single Cargo compiler lock. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete native Bevy 0.16 3D office world in `crates/station-3d` achieving 100% spatial, visual, and interactive parity across all 55 P3 Oracle files (`cozy` 9-desk, `pro` 12-desk, and `hq` 44-desk floor plans, furniture, all 22 props, lighting, isometric + first-person cameras, picking/hover rings) while staying well under hard Windows Private RAM budgets (`Pro ≤ 300 MB`, `HQ ≤ 380 MB`).

**Architecture:** Use a shared `WorldPalette` resource (`Handle<Mesh>` unit primitives + deduplicated `Handle<StandardMaterial>` cache) so 44 desks, walls, partitions, furniture, and 22 props share ~12 GPU meshes and ~60 materials. Upload every `station-canvas2d::RgbaSurface` with `RenderAssetUsages::RENDER_WORLD` so CPU pixel memory drops immediately. Tag all 3D entities with `WorldEntity` for instant true unload on layout change / 2D switch / Tray minimize.

**Tech Stack:** Bevy 0.16 (`default-features = false`), `station-canvas2d`, `station-core`.

**Spec:** `docs/superpowers/specs/2026-11-25-agents-station-design.md`

## Global Constraints

- Minimal Bevy 0.16 feature set unchanged; Vulkan default backend + `MemoryHints::MemoryUsage` + `Msaa::Off` + bounded shadow maps (`1024`/`256`).
- **Hard RAM Budgets:** `3D Pro ≤ 300 MB` Private Working Set; `3D HQ (44 desks) ≤ 380 MB` Private Working Set.
- **Zero CPU texture retention:** All static textures uploaded via `RenderAssetUsages::RENDER_WORLD`.
- **Single Compiler Lock:** Only one `cargo` process at a time; target on `D:\mystation\cargo-target`.
- **Parity Ledger:** All 55 P3 files marked `[x]` in `PARITY_LEDGER.md` upon Gate P3 completion.

## Review Focus

1. **Exact Oracle spatial parity:** `Cozy` (`16×12m`, 9 desks), `Pro` (`22×17m`, 12 desks in 3 pods + director/meeting/library/pantry/lounge/server/lobby/restrooms), `HQ` (`22×34m`, 44 desks across 3 core pods + 4 wing department rooms) match Oracle coordinates without clipping or floating furniture.
2. **True Unload on layout switch (`Cozy ↔ Pro ↔ HQ`):** Despawning `WorldEntity` leaves 0 orphaned 3D entities and releases transient GPU images.
3. **22 props completeness & animation safety:** Aquarium, arcade, billiards, bins, cat, cups, dev-servers, duty-board, game-tv, karaoke, microwave, neon, printer, shredder, string-lights, switches, trinkets, tv-games, live stats TV, robot vacuum, whiteboard, and coffee corner all spawn cleanly in their exact layout positions and animate without NaN transforms.
4. **Live `station-canvas2d` integration:** Desk screens, whiteboard, duty board, live stats TV, arcade screen/marquee, karaoke band/QR/screen, and game-TV display crisp sRGB textures on their 3D quads.
5. **Visual & RAM Gate Proof:** `--capture-scene cozy`, `--capture-scene pro`, `--capture-scene hq` produce verified PNGs inspected via `read_file` with Windows Private RAM measured below hard targets (`Pro ≤ 300 MB`, `HQ ≤ 380 MB`).

---

### Task 1: `WorldPalette`, Layouts (`Cozy`/`Pro`/`Hq`), Camera Rig & Day/Night Lighting

**Files:**
- Modify: `crates/station-3d/Cargo.toml` (add `station-canvas2d`)
- Create: `crates/station-3d/src/{palette.rs,layout.rs,camera.rs,lights.rs}`
- Modify: `crates/station-3d/src/lib.rs`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Resource)] pub enum OfficeLayout { Cozy, Pro, Hq }`
  - `pub struct LayoutSpec` (exact coordinates from `world/layout.js`: `box`, `center`, `shelf`, `globe`, `board`, `racks`, `tv`, `neon`, `rocket`, `coffee`, `aq`, `arc`, `prn`, `sofa`, `game_tv`, `bil`, `ktv`, `pods`, `desks`, `depts`)
  - `pub struct WorldPalette` (`box_mesh`, `cyl_mesh`, `sphere_mesh`, `plane_mesh`, `mat(&mut materials, hex, roughness, metallic)`, `emissive_mat(...)`, `glass_mat(...)`, `upload_surface(&mut images, surface)`)
  - `pub struct CameraRig` (isometric `Projection::Orthographic` matching Oracle `HOME`/`BASE_ZOOM`, left-drag pan, right-drag orbit, scroll zoom, `F` first-person toggle)
  - `pub struct LightingState` (day/night transition, sun shadow rig, room lamps)

- [ ] **Step 1: Write unit tests for all 3 layout desk counts (`9`, `12`, `44`), `LayoutSpec::inside()`, `REACH` bounds, and `WorldPalette` material deduplication**
- [ ] **Step 2: Implement `palette.rs`, `layout.rs`, `camera.rs`, `lights.rs`**
- [ ] **Step 3: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 4: Commit**

```bash
git add crates/station-3d/
git commit -m "feat(3d): add shared WorldPalette, Cozy/Pro/HQ layouts, camera rig, and lighting"
```

---

### Task 2: Architectural Shells (`Cozy` Room, `Pro` Company Floor, `HQ` Wing, Windows, Terrace & Entrance)

**Files:**
- Create: `crates/station-3d/src/{rooms.rs,entrance.rs}`
- Modify: `crates/station-3d/src/lib.rs`

**Interfaces:**
- Consumes: `WorldPalette`, `LayoutSpec`, `station_canvas2d`
- Produces:
  - `pub fn spawn_office_shell(...)`: builds exact Cozy wood floor + sage wainscoting + zone rugs, OR Pro 22×17m company floor (director walnut office, meeting room + Sprint Review screen, library + ladder, pantry + subway tiles/fridge/sink/tables, lounge, server raised tiles, lobby stone, restrooms mosaic, glass partitions, open double doors), PLUS HQ wing (`z: 9..26`, corridors A/B, 4 glass-walled department rooms), outdoor terrace, potted plants, window frames, and entrance lift/stairs!

- [ ] **Step 1: Implement `rooms.rs` & `entrance.rs` with unit tests verifying shell entity counts and `WorldEntity` tagging**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

```bash
git add crates/station-3d/
git commit -m "feat(3d): build Cozy, Pro, and HQ office shells, partitions, terrace, and entrance"
```

---

### Task 3: Desks (9/12/44), Dual Monitors, Rolling Chairs & Core Furniture

**Files:**
- Create: `crates/station-3d/src/{desks.rs,furniture.rs}`
- Modify: `crates/station-3d/src/lib.rs`

**Interfaces:**
- Consumes: `WorldPalette`, `LayoutSpec`, `station_canvas2d::office_surfaces::draw_desk_screen`, `boards_and_social::draw_whiteboard`
- Produces:
  - `pub fn spawn_desks(...)`: spawns all 9 / 12 / 44 desks using shared unit meshes + live `station-canvas2d` screen textures (`Code`/`Read`/`Term`/`Web`/`Chat`/`Wait`/`Sleep`/`Off`), laptops, secondary monitors & drawers on Pro/HQ, 5-spoke office chairs, desk lamps, and `DeskSlot` components.
  - `pub fn spawn_furniture(...)`: whiteboard + sticky slots, coffee corner + espresso machine + donuts, meeting table + seats, 3 server racks + 84 LEDs + 6 spinning fans (`ServerFan`), bookshelf, lounge sofa, rocket sculpture, floor lamps.

- [ ] **Step 1: Implement `desks.rs` and `furniture.rs` with unit tests verifying exact desk/chair/fan/rack counts across Cozy/Pro/HQ**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

```bash
git add crates/station-3d/
git commit -m "feat(3d): add 9/12/44 desk pods, live canvas2d screens, chairs, and office furniture"
```

---

### Task 4: All 22 Interactive Props, Animations & Picking / Hover Ring Interaction

**Files:**
- Create: `crates/station-3d/src/{props.rs,interact.rs}`
- Modify: `crates/station-3d/src/lib.rs`

**Interfaces:**
- Consumes: `WorldPalette`, `LayoutSpec`, `station_canvas2d::{office_surfaces,boards_and_social}`
- Produces:
  - `pub fn spawn_all_props(...)`: builds all 22 props (`aquarium` + swimming fish, `arcade` + marquee + invaders, `billiards` table + balls, `bins`, `cat` office tabby, `cups`, `dev-servers` mini racks, `duty-board` + sink cup pile, `game-tv` + PS5 console + 2 controllers + bias glow, `karaoke` glass/purple cabin + `KARAOKE` neon band + QR badge + screen + mics, `microwave`, `neon` sign, `printer`, `shredder`, `string-lights`, `switches`, `trinkets`, `tv` live stats wall TV, `vacuum` robot vacuum).
  - `pub fn animate_props_system(...)`: spins server fans, patrols robot vacuum, swims aquarium fish, blinks arcade/LEDs, purrs/breathes office cat.
  - `pub struct InteractPlugin`: hover ring, `InteractiveProp` registry, click selection event (`PropActivated`), and place-card/place-chip state (`PlaceId`).

- [ ] **Step 1: Implement `props.rs` and `interact.rs` with unit tests verifying all 22 prop kinds and animation step stability**
- [ ] **Step 2: Run `cargo test -p station-3d` and verify PASS**
- [ ] **Step 3: Commit**

```bash
git add crates/station-3d/
git commit -m "feat(3d): implement all 22 animated office props and interactive picking rings"
```

---

### Task 5: Wire `OfficeWorldPlugin` into `station-app`, Capture Cozy/Pro/HQ PNGs & RAM Certification Gate

**Files:**
- Modify: `crates/station-3d/src/lib.rs` (`OfficeWorldPlugin` supporting layout selection + `unload_office_world` true unload)
- Modify: `crates/station-app/src/{main.rs,capture.rs}` (support `--capture-scene cozy`, `pro`, `hq` in addition to `smoke`)
- Modify: `PARITY_LEDGER.md` (mark all 55 P3 files `[x]`)

- [ ] **Step 1: Wire `OfficeWorldPlugin` into `station-app` and build `agents-station.exe`**
- [ ] **Step 2: Capture `artifacts/g3-cozy.png`, `artifacts/g3-pro.png`, `artifacts/g3-hq.png` and measure Windows Private Working Set for Pro (`≤ 300 MB`) and HQ (`≤ 380 MB`)**
- [ ] **Step 3: Visually inspect all 3 PNGs via `read_file`; fix any camera framing, lighting, or layout issues until crisp**
- [ ] **Step 4: Mark all 55 P3 files `[x]` in `PARITY_LEDGER.md`, run `cargo test --workspace`, commit, and push Gate P3 to `origin/main`**
