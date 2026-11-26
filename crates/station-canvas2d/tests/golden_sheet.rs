use station_canvas2d::boards_and_social::{
    DutyBoardInput, DutyRowInput, PhotostripInput, StripPerson, WhiteboardInput, WhiteboardTask,
    draw_duty_board, draw_photostrip, draw_sticky_note, draw_whiteboard,
};
use station_canvas2d::office_surfaces::{
    ScreenKind, TvStatsInput, draw_arcade_frame, draw_arcade_marquee, draw_desk_screen,
    draw_karaoke_band, draw_karaoke_qr_badge, draw_stats_tv,
};
use station_canvas2d::street_surfaces::{
    SignalColor, draw_atm_atlas, draw_ev_cabinet_atlas, draw_photo_studio_atlas,
    draw_pickleball_atlas, draw_shop_cell, draw_traffic_countdown,
};
use station_canvas2d::{BrandRegistry, Canvas2d, FontEngine, TextAlign, hex};
use std::path::PathBuf;

#[test]
fn generates_golden_canvas2d_contact_sheet_png() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let assets = root.join("assets");
    let out = root.join("artifacts/g2-canvas2d-sheet.png");

    let mut fonts = FontEngine::from_assets_dir(&assets);
    let mut brands = BrandRegistry::new(assets);

    let mut sheet = Canvas2d::new(1600, 1200);
    sheet.fill(hex("#12151E"));
    fonts.draw_text(
        &mut sheet,
        "Agents Station · Gate P2 Golden 2D Procedural Texture Sheet",
        24.0,
        14.0,
        24.0,
        hex("#F5F0E8"),
        TextAlign::Left,
        None,
        true,
        false,
    );
    fonts.draw_text(
        &mut sheet,
        "Tiếng Việt: Ă Ắ Ặ Đ Ơ Ư Ỹ · ผู้จัดการ · ภาษาไทย · ≤ 300 MB",
        1576.0,
        18.0,
        18.0,
        hex("#A8D5BA"),
        TextAlign::Right,
        Some(760.0),
        false,
        false,
    );

    // Row 1: Desk screens + Live TV + Arcade
    let code = draw_desk_screen(ScreenKind::Code, &mut fonts);
    let wait = draw_desk_screen(ScreenKind::Wait, &mut fonts);
    let tv = draw_stats_tv(
        &TvStatsInput {
            title: "AGENTS STATION · LIVE".into(),
            clock_hhmm: "15:48".into(),
            ram_pct: Some(42),
            ram_critical: false,
            cpu_pct: 18,
            working: 4,
            waiting: 1,
            resting: 2,
            today_summary: "68 prompts · 31 cmds".into(),
            plan_pct: Some(54),
            pulse_30m: [1, 2, 0, 3, 5, 4, 2, 1, 6, 8, 3, 2, 4, 5, 7, 9, 6, 4, 2, 3, 5, 6, 8, 4, 3, 2, 5, 7, 6, 4],
        },
        &mut fonts,
    );
    let arc = draw_arcade_frame(2.4, false, &mut fonts);
    let marq = draw_arcade_marquee(&mut fonts);
    let ktv_band = draw_karaoke_band("KARAOKE", &mut fonts);
    let ktv_qr = draw_karaoke_qr_badge(&mut fonts);

    sheet.blit_rgba(&code, 24, 56, 200, 250);
    sheet.blit_rgba(&wait, 240, 56, 200, 250);
    sheet.blit_rgba(&tv, 456, 56, 440, 240);
    sheet.blit_rgba(&marq, 916, 56, 240, 60);
    sheet.blit_rgba(&arc, 916, 126, 160, 120);
    sheet.blit_rgba(&draw_traffic_countdown(18, SignalColor::Green), 1088, 126, 96, 60);
    sheet.blit_rgba(&draw_traffic_countdown(9, SignalColor::Red), 1088, 194, 96, 60);
    sheet.blit_rgba(&ktv_band, 1200, 56, 376, 48);
    sheet.blit_rgba(&ktv_qr, 1200, 114, 240, 120);

    // Row 2: Whiteboard + Sticky + Duty Board + Photostrip
    let wb = draw_whiteboard(
        &WhiteboardInput {
            title: "Whiteboard".into(),
            yours_title: "Yours".into(),
            streamer: false,
            tasks: vec![
                WhiteboardTask {
                    name: "Claude".into(),
                    color: hex("#D97757"),
                    state: "doing".into(),
                    prompt: "Tối ưu RAM xuống ≤ 300 MB với Rust + Bevy".into(),
                    todos_done: 4,
                    todos_total: 6,
                },
                WhiteboardTask {
                    name: "Antigravity".into(),
                    color: hex("#5E9BF0"),
                    state: "done".into(),
                    prompt: "Certify Gate P1 (6.82 MB Private WS)".into(),
                    todos_done: 3,
                    todos_total: 3,
                },
            ],
            open_todos: 2,
            total_todos: 3,
        },
        &mut fonts,
    );
    let sticky = draw_sticky_note("Mua cà phê sữa đá cho cả team", false, hex("#FFE787"), false, &mut fonts);
    let duty = draw_duty_board(
        &DutyBoardInput {
            title: "TRỰC NHẬT".into(),
            date: "25/11".into(),
            off_label: None,
            rows: vec![
                DutyRowInput { chore_key: "cups".into(), chore_label: "Rửa ly".into(), person: "Mai".into(), done_at: Some("11:30".into()), busy: false },
                DutyRowInput { chore_key: "trash".into(), chore_label: "Đổ rác".into(), person: "Long".into(), done_at: None, busy: true },
                DutyRowInput { chore_key: "plants".into(), chore_label: "Tưới cây".into(), person: "An".into(), done_at: None, busy: false },
            ],
            owed_footer: Some("Nợ hôm qua: Lau bàn (Huy)".into()),
        },
        &mut fonts,
    );
    let strip = draw_photostrip(
        &PhotostripInput {
            tag: "v1.0.0".into(),
            day: "25/11".into(),
            seed: 1,
            people: vec![
                StripPerson { hair: hex("#3B2618"), skin: hex("#F5D0B5"), shirt: hex("#E8865F") },
                StripPerson { hair: hex("#1E1E24"), skin: hex("#EBC3A0"), shirt: hex("#5F8FD8") },
                StripPerson { hair: hex("#5A3A2A"), skin: hex("#F7DAC4"), shirt: hex("#2E9E62") },
            ],
        },
        &mut fonts,
    );

    sheet.blit_rgba(&wb, 24, 326, 640, 320);
    sheet.blit_rgba(&sticky, 680, 326, 150, 150);
    sheet.blit_rgba(&duty, 846, 326, 470, 320);
    sheet.blit_rgba(&strip, 1336, 326, 110, 320);

    // Row 3: Vietnamese Street Shops (with real brand SVGs!) + ATM + V-Green EV + Photo Studio + Pickleball
    let gs25 = draw_shop_cell("gs25", &mut brands, &mut fonts);
    let circlek = draw_shop_cell("circlek", &mut brands, &mut fonts);
    let pho = draw_shop_cell("pho", &mut brands, &mut fonts);
    let atm = draw_atm_atlas(&mut fonts);
    let ev = draw_ev_cabinet_atlas(&mut fonts);
    let photo = draw_photo_studio_atlas(&mut fonts);
    let pickle = draw_pickleball_atlas(&mut fonts);

    sheet.blit_rgba(&gs25, 24, 672, 500, 250);
    sheet.blit_rgba(&circlek, 544, 672, 500, 250);
    sheet.blit_rgba(&pho, 1064, 672, 500, 250);

    sheet.blit_rgba(&atm, 24, 938, 240, 240);
    sheet.blit_rgba(&ev, 284, 938, 384, 240);
    sheet.blit_rgba(&photo, 688, 938, 440, 240);
    sheet.blit_rgba(&pickle, 1148, 938, 428, 240);

    let surf = sheet.into_surface();
    surf.save_png(&out).unwrap();
    assert!(out.exists());
    assert!(surf.non_background_ratio(hex("#12151E")) > 0.7);
}
