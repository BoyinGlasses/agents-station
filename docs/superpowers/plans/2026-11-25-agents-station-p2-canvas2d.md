# Agents Station — Plan 2: `station-canvas2d` Procedural 2D Textures & Public Assets

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task under a single Cargo compiler lock. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `crates/station-canvas2d` replacing all 25+ HTML5 `CanvasTexture` generators and all 17 `public/` assets (`brands/*.{svg,png}`, `thumbs/*.jpg`, `vehicles/cars.json`, `icon-512.png`, `sw.js`) using pure Rust (`tiny-skia` + `cosmic-text` + `resvg`), outputting tight sRGB RGBA8 buffers ready for `RenderAssetUsages::RENDER_WORLD` GPU upload and immediate CPU drop.

**Architecture:** Keep `station-canvas2d` completely free of Bevy so it compiles and tests in seconds. Expose deterministic `RgbaSurface { width, height, rgba: Vec<u8> }` generators grouped by domain (`canvas`, `text`, `brands`, `office_surfaces`, `boards_and_social`, `street_surfaces`), plus a Golden PNG contact-sheet test that renders every surface to `artifacts/g2-canvas2d-sheet.png` for visual inspection.

**Tech Stack:** Rust 2024, `tiny-skia 0.11`, `cosmic-text 0.13`, `resvg 0.45`, `image 0.25` (PNG + JPEG), `serde_json`, `anyhow`.

**Spec:** `docs/superpowers/specs/2026-11-25-agents-station-design.md`

## Global Constraints

- `CARGO_TARGET_DIR` pinned to `D:\mystation\cargo-target`; `[build] jobs = 2`; **Single Compiler Lock** (never run two `cargo` invocations concurrently).
- **Zero CPU texture retention:** Generators return owned `Vec<u8>` RGBA buffers; callers upload with `RenderAssetUsages::RENDER_WORLD` and drop CPU memory immediately.
- **Full Vietnamese & Thai glyph shaping:** All text rendering MUST use the bundled TTF fonts in `assets/fonts/` (`BeVietnamPro`, `JetBrainsMono`, `PlexThaiLooped`), never missing-glyph tofu boxes.
- **Visual Parity:** Colors, layout proportions, and copy match Golden Oracle (`d:\mystation\my-station`) verbatim (`screens.js`, `tv.js`, `arcade.js`, `karaoke.js`, `duty-board.js`, `whiteboard.js`, `photostrips.js`, `shops.js`, `street-atm.js`, `street-ev.js`, `street-photo.js`, `street-pickle.js`, `street-signals.js`).
- **Parity Ledger:** Mark all 17 `public/` files `[x]` in `PARITY_LEDGER.md` upon Gate P2 completion.

## Review Focus

1. **Vietnamese diacritics & Thai rendering on 2D canvases:** `TRỰC NHẬT`, `BÚN BÒ HUẾ`, `CHỤP ẢNH HÀN QUỐC`, `5.000đ`, `200.000–300.000đ/giờ`, and Thai strings render cleanly without clipping or tofu glyphs (tested in Task 1 + visually verified in Task 5).
2. **SVG crop & aspect scaling without distortion:** All 6 brand SVGs (`7-eleven`, `circle-k`, `grab`, `gs25`, `ministop`, `xanh-sm`) rasterize non-empty with valid alpha and exact requested dimensions (tested in Task 2).
3. **Deterministic seeded screens & zero panics on extreme inputs:** Empty task lists, long Vietnamese prompts (`fit_text` ellipsis), 0% / 100% / >100% CPU/RAM stats, and streamer redaction mode never panic or write out-of-bounds (tested in Tasks 3 & 4).
4. **Memory discipline:** No unbounded global pixel cache; brand logo cache stores compact bounded RGBA surfaces only when loaded and can be purged via `BrandRegistry::clear()` (tested in Task 2).
5. **Visual Golden Proof:** Contact sheet `artifacts/g2-canvas2d-sheet.png` displays screens, TV, arcade, karaoke, whiteboard, duty board, photostrip, ATM, V-Green EV, photo studio, pickleball, traffic countdown, and shop facades side-by-side and is inspected directly via `read_file` (Task 5).

---

### Task 1: `Canvas2d` Drawing Primitives & `FontEngine` (Vietnamese / Thai / Mono)

**Files:**
- Modify: `Cargo.toml` (add `tiny-skia`, `cosmic-text`, `resvg`, enable `jpeg` on `image` for layout thumbs)
- Modify: `crates/station-canvas2d/Cargo.toml`
- Create: `crates/station-canvas2d/src/{canvas.rs,text.rs}`
- Modify: `crates/station-canvas2d/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct RgbaSurface { pub width: u32, pub height: u32, pub rgba: Vec<u8> }` (`save_png(&self, path: &Path) -> Result<()>`, `non_background_ratio(&self, bg: [u8; 4]) -> f32`)
  - `pub struct Canvas2d` (`new(w: u32, h: u32) -> Self`, `fill_rect`, `round_rect`, `stroke_rect`, `fill_circle`, `fill_ellipse`, `stroke_line`, `linear_gradient_rect`, `radial_gradient_circle`, `blit_rgba`, `into_surface(self) -> RgbaSurface`)
  - `pub enum TextAlign { Left, Center, Right }`
  - `pub struct FontEngine` (`from_assets_dir(assets_dir: &Path) -> Self`, `draw_text(&mut self, canvas: &mut Canvas2d, text: &str, x: f32, y: f32, size_px: f32, color: [u8; 4], align: TextAlign, max_width: Option<f32>, bold: bool, mono: bool)`)

- [ ] **Step 1: Write unit tests for `Canvas2d` primitives, gradients, and Vietnamese/Thai `FontEngine` text rasterization**
- [ ] **Step 2: Implement `canvas.rs` and `text.rs`**
- [ ] **Step 3: Run `cargo test -p station-canvas2d` and verify PASS**
- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/station-canvas2d/
git commit -m "feat(canvas2d): add tiny-skia Canvas2d primitives and cosmic-text Vietnamese/Thai engine"
```

---

### Task 2: Brand SVG/PNG Rasterizer & All 17 `public/` Assets Registry

**Files:**
- Create: `crates/station-canvas2d/src/brands.rs`
- Modify: `crates/station-canvas2d/src/lib.rs`

**Interfaces:**
- Consumes: `assets/brands/*`, `assets/thumbs/*`, `assets/vehicles/cars.json`, `assets/icon-512.png`, `assets/vendor/qrcode.min.js`, `assets/sw.js`
- Produces:
  - `pub struct BrandRegistry` (`new(assets_dir: PathBuf) -> Self`, `load_brand_rgba(&mut self, file: &str, max_w: u32, max_h: u32) -> Result<RgbaSurface>`, `load_thumb_rgba(&self, layout: &str) -> Result<RgbaSurface>`, `load_vehicle_catalog(&self) -> Result<serde_json::Value>`, `load_app_icon(&self) -> Result<RgbaSurface>`, `clear(&mut self)`)

- [ ] **Step 1: Write tests verifying all 6 brand SVGs, 2 brand PNGs, 3 layout JPEG thumbs, `cars.json`, `icon-512.png`, and `clear()` memory drop**
- [ ] **Step 2: Implement `brands.rs` using `resvg` + `image`**
- [ ] **Step 3: Run `cargo test -p station-canvas2d brands` and verify PASS**
- [ ] **Step 4: Commit**

```bash
git add crates/station-canvas2d/
git commit -m "feat(canvas2d): add resvg brand rasterizer and public asset registry"
```

---

### Task 3: Office Interior Procedural Surfaces (Desk Screens, TV, Arcade, Karaoke, Game-TV, Dev-Servers)

**Files:**
- Create: `crates/station-canvas2d/src/office_surfaces.rs`
- Modify: `crates/station-canvas2d/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum ScreenKind { Off, Code, Read, Term, Web, Chat, Wait, Sleep }`
  - `pub fn draw_desk_screen(kind: ScreenKind, fonts: &mut FontEngine) -> RgbaSurface` (`512×640`, exact Oracle seeded RNG + colors)
  - `pub fn draw_stats_tv(stats: &TvStatsInput, fonts: &mut FontEngine) -> RgbaSurface` (`512×280`)
  - `pub fn draw_arcade_frame(t: f32, insert_coin: bool, fonts: &mut FontEngine) -> RgbaSurface` (`96×72`)
  - `pub fn draw_arcade_marquee(fonts: &mut FontEngine) -> RgbaSurface` (`256×64`)
  - `pub fn draw_karaoke_band(text: &str, fonts: &mut FontEngine) -> RgbaSurface` (`512×64`)
  - `pub fn draw_karaoke_qr_badge(fonts: &mut FontEngine) -> RgbaSurface` (`256×128`)
  - `pub fn draw_karaoke_screen(title: &str, lyric: &str, progress: f32, fonts: &mut FontEngine) -> RgbaSurface` (`512×288`)
  - `pub fn draw_game_tv_standby(fonts: &mut FontEngine) -> RgbaSurface` (`384×216`) & `pub fn draw_radial_glow() -> RgbaSurface` (`128×128`)
  - `pub fn draw_dev_server_badge(port: Option<u16>, fonts: &mut FontEngine) -> RgbaSurface` (`96×32`)

- [ ] **Step 1: Write unit tests for all 8 screen kinds + TV/arcade/karaoke/game-tv/dev-server surfaces**
- [ ] **Step 2: Implement `office_surfaces.rs`**
- [ ] **Step 3: Run `cargo test -p station-canvas2d office_surfaces` and verify PASS**
- [ ] **Step 4: Commit**

```bash
git add crates/station-canvas2d/
git commit -m "feat(canvas2d): port desk screens, stats TV, arcade, karaoke, and dev-server surfaces"
```

---

### Task 4: Boards & Social Surfaces (Whiteboard, Sticky Notes, Duty Board, Photostrips, Avatars)

**Files:**
- Create: `crates/station-canvas2d/src/boards_and_social.rs`
- Modify: `crates/station-canvas2d/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub fn draw_whiteboard(input: &WhiteboardInput, fonts: &mut FontEngine) -> RgbaSurface` (`1024×512`, supports `streamer` redaction bars)
  - `pub fn draw_sticky_note(text: &str, done: bool, color: [u8; 4], streamer: bool, fonts: &mut FontEngine) -> RgbaSurface` (`192×192`)
  - `pub fn draw_duty_board(input: &DutyBoardInput, fonts: &mut FontEngine) -> RgbaSurface` (`512×349`)
  - `pub fn draw_photostrip(input: &PhotostripInput, fonts: &mut FontEngine) -> RgbaSurface` (`128×384`)
  - `pub fn draw_chat_avatar_initials(name: &str, key: &str, size: u32, fonts: &mut FontEngine) -> RgbaSurface`

- [ ] **Step 1: Write unit tests covering normal + streamer mode + empty + Vietnamese text on whiteboard, stickies, duty board, photostrip, and avatars**
- [ ] **Step 2: Implement `boards_and_social.rs`**
- [ ] **Step 3: Run `cargo test -p station-canvas2d boards_and_social` and verify PASS**
- [ ] **Step 4: Commit**

```bash
git add crates/station-canvas2d/
git commit -m "feat(canvas2d): port whiteboard, sticky notes, duty board, photostrips, and avatars"
```

---

### Task 5: Street & Shop Procedural Surfaces + Golden PNG Contact-Sheet Visual Gate

**Files:**
- Create: `crates/station-canvas2d/src/street_surfaces.rs`
- Create: `crates/station-canvas2d/tests/golden_sheet.rs`
- Modify: `crates/station-canvas2d/src/lib.rs`, `PARITY_LEDGER.md`

**Interfaces:**
- Produces:
  - `pub fn draw_shop_cell(chain_id: &str, brands: &mut BrandRegistry, fonts: &mut FontEngine) -> RgbaSurface` (`1024×512` covering Circle K, GS25, 7-Eleven, Ministop, WinMart+, Bách Hóa Xanh, Mixue, Highlands, TGDD, Katinat, Phúc Long, Long Châu, ĐMX, Cộng, and all 13 regional Vietnamese eateries)
  - `pub fn draw_atm_atlas(fonts: &mut FontEngine) -> RgbaSurface` (`512×512`, Vietcombank ATM fascia/side/band/screen/header/note)
  - `pub fn draw_ev_cabinet_atlas(fonts: &mut FontEngine) -> RgbaSurface` (`1024×640`, V-Green battery-swap lockers + LEZ sign)
  - `pub fn draw_photo_studio_atlas(fonts: &mut FontEngine) -> RgbaSurface` (`1024×512`, neon `Chụp ảnh Hàn Quốc` + price board + sample strips)
  - `pub fn draw_pickleball_atlas(fonts: &mut FontEngine) -> RgbaSurface` (`1024×512`, windscreen + gate board + perforated ball)
  - `pub fn draw_traffic_countdown(seconds: u32, color: SignalColor) -> RgbaSurface` (`64×40` 7-segment LEDs)

- [ ] **Step 1: Write `street_surfaces.rs` and `tests/golden_sheet.rs` compositing all representative textures into `artifacts/g2-canvas2d-sheet.png`**
- [ ] **Step 2: Run `cargo test -p station-canvas2d` and inspect `artifacts/g2-canvas2d-sheet.png` directly via `read_file`**
- [ ] **Step 3: Fix any typography/clipping/visual issues until crisp; mark all 17 P2 files `[x]` in `PARITY_LEDGER.md`**
- [ ] **Step 4: Commit and push Gate P2 to `origin/main`**

```bash
git add crates/station-canvas2d/ PARITY_LEDGER.md
git commit -m "feat(canvas2d): complete street surfaces and certify G2 procedural texture visual gate"
git push origin main
```
