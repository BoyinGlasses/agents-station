use crate::brands::BrandRegistry;
use crate::canvas::{Canvas2d, RgbaSurface, hex, hex_rgba};
use crate::text::{FontEngine, TextAlign};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalColor {
    Red,
    Green,
    Amber,
}

const SEGMENTS: [&str; 10] = [
    "abcdef", "bc", "abdeg", "abcdg", "bcfg", "acdfg", "acdefg", "abc", "abcdefg", "abcdfg",
];

fn draw_seven_seg(c: &mut Canvas2d, digit: u32, x: f32, y: f32, w: f32, h: f32, on: [u8; 4], off: [u8; 4]) {
    let t = (w * 0.2).round().max(2.0);
    let segs = SEGMENTS[(digit as usize).min(9)];
    let half = h * 0.5;
    let map = [
        ('a', x + t, y, w - 2.0 * t, t),
        ('b', x + w - t, y + t, t, half - t * 1.5),
        ('c', x + w - t, y + half + t * 0.5, t, half - t * 1.5),
        ('d', x + t, y + h - t, w - 2.0 * t, t),
        ('e', x, y + half + t * 0.5, t, half - t * 1.5),
        ('f', x, y + t, t, half - t * 1.5),
        ('g', x + t, y + half - t * 0.5, w - 2.0 * t, t),
    ];
    for (k, rx, ry, rw, rh) in map {
        c.fill_rect(rx, ry, rw, rh, if segs.contains(k) { on } else { off });
    }
}

/// Exact Oracle `src/office/world/backdrops/street-signals.js` 64×40 countdown LED cell.
pub fn draw_traffic_countdown(seconds: u32, color: SignalColor) -> RgbaSurface {
    let mut c = Canvas2d::new(64, 40);
    c.fill(hex("#050505"));
    let (on, off) = match color {
        SignalColor::Red => (hex("#FF3322"), hex("#3A0D0A")),
        SignalColor::Green => (hex("#33FF66"), hex("#0B3316")),
        SignalColor::Amber => (hex("#FFC422"), hex("#3A2A08")),
    };
    let v = seconds.min(99);
    draw_seven_seg(&mut c, v / 10, 9.0, 5.0, 20.0, 30.0, on, off);
    draw_seven_seg(&mut c, v % 10, 35.0, 5.0, 20.0, 30.0, on, off);
    c.into_surface()
}

const QR_QUIET_ZONE: usize = 4;

/// Renders a real ISO-18004 QR code with a 4-module quiet zone and crisp nearest-neighbor scaling.
/// Returns `(rgba_bytes, width_px, height_px)`.
pub fn render_qr_rgba(payload: &str, size_px: u32) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let code = qrcode::QrCode::new(payload.as_bytes())?;
    let mods = code.width();
    let total_mods = mods + QR_QUIET_ZONE * 2;
    let scale = (size_px as usize / total_mods).max(1);
    let side = total_mods * scale;
    let mut rgba = vec![255u8; side * side * 4];
    let dark = hex("#111111");
    for my in 0..mods {
        for mx in 0..mods {
            if code[(mx, my)] == qrcode::Color::Dark {
                let x0 = (mx + QR_QUIET_ZONE) * scale;
                let y0 = (my + QR_QUIET_ZONE) * scale;
                for dy in 0..scale {
                    let row = (y0 + dy) * side;
                    for dx in 0..scale {
                        let idx = (row + x0 + dx) * 4;
                        rgba[idx..idx + 4].copy_from_slice(&dark);
                    }
                }
            }
        }
    }
    Ok((rgba, side as u32, side as u32))
}

/// Draws a real ISO-18004 QR code onto a `Canvas2d` with a 4-module white quiet zone.
pub fn draw_qr_block(c: &mut Canvas2d, x: f32, y: f32, s: f32, payload: &str) -> anyhow::Result<()> {
    let code = qrcode::QrCode::new(payload.as_bytes())?;
    c.fill_rect(x, y, s, s, hex("#FFFFFF"));
    let mods = code.width();
    let total_mods = mods + QR_QUIET_ZONE * 2;
    let cell = s / total_mods as f32;
    let dark = hex("#111111");
    for my in 0..mods {
        for mx in 0..mods {
            if code[(mx, my)] == qrcode::Color::Dark {
                c.fill_rect(
                    x + (mx + QR_QUIET_ZONE) as f32 * cell,
                    y + (my + QR_QUIET_ZONE) as f32 * cell,
                    cell,
                    cell,
                    dark,
                );
            }
        }
    }
    Ok(())
}

/// Exact Oracle `src/office/world/backdrops/street-atm.js` 512×512 Vietcombank ATM atlas.
pub fn draw_atm_atlas(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 512);
    let vcb = hex("#009A49");
    let vcb_dark = hex("#00703C");
    let vcb_light = hex("#8DC63F");

    // Fascia [0, 0, 512, 144]
    c.linear_gradient_rect(0.0, 0.0, 512.0, 144.0, (0.0, 0.0), (0.0, 144.0), &[(0.0, hex("#0BB45A")), (1.0, vcb_dark)]);
    c.fill_rect(0.0, 0.0, 512.0, 10.0, [255, 255, 255, 40]);
    c.fill_circle(74.0, 72.0, 46.0, hex("#FFFFFF"));
    c.fill_polygon(&[(74.0, 104.0), (46.0, 50.0), (62.0, 46.0), (74.0, 74.0), (86.0, 46.0), (102.0, 50.0)], vcb_light);
    fonts.draw_text(&mut c, "Vietcombank", 136.0, 42.0, 56.0, hex("#FFFFFF"), TextAlign::Left, Some(360.0), true, false);

    // Side [0, 144, 512, 144]
    c.linear_gradient_rect(0.0, 144.0, 512.0, 144.0, (0.0, 144.0), (0.0, 288.0), &[(0.0, hex("#0BB45A")), (1.0, vcb_dark)]);
    fonts.draw_text(&mut c, "ATM", 204.0, 174.0, 84.0, hex("#FFFFFF"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "24/7", 400.0, 198.0, 40.0, hex("#FFFFFF"), TextAlign::Center, None, true, false);

    // Frosted band [0, 288, 512, 64]
    c.fill_rect(0.0, 288.0, 512.0, 64.0, hex_rgba("#F0F8F4", 235));
    c.fill_rect(0.0, 294.0, 512.0, 6.0, vcb);
    c.fill_rect(0.0, 340.0, 512.0, 6.0, vcb);
    fonts.draw_text(&mut c, "Vietcombank · ATM", 256.0, 306.0, 26.0, vcb_dark, TextAlign::Center, Some(490.0), true, false);

    // Screen [0, 352, 192, 144]
    c.fill_rect(0.0, 352.0, 192.0, 144.0, hex("#EAF6EE"));
    c.fill_rect(0.0, 352.0, 192.0, 30.0, vcb);
    fonts.draw_text(&mut c, "Vietcombank", 14.0, 358.0, 17.0, hex("#FFFFFF"), TextAlign::Left, None, true, false);
    fonts.draw_text(&mut c, "Xin chào Quý khách", 96.0, 396.0, 16.0, hex("#16301F"), TextAlign::Center, Some(180.0), true, false);
    fonts.draw_text(&mut c, "Vui lòng đưa thẻ", 96.0, 422.0, 13.0, hex("#16301F"), TextAlign::Center, None, false, false);
    fonts.draw_text(&mut c, "hoặc quét mã QR", 96.0, 440.0, 13.0, hex("#16301F"), TextAlign::Center, None, false, false);
    c.round_rect(74.0, 464.0, 44.0, 26.0, 4.0, vcb_light);
    c.fill_rect(82.0, 472.0, 10.0, 8.0, hex("#F7C600"));

    // Header [192, 352, 320, 80]
    c.linear_gradient_rect(192.0, 352.0, 320.0, 80.0, (192.0, 352.0), (192.0, 432.0), &[(0.0, hex("#0BB45A")), (1.0, vcb_dark)]);
    fonts.draw_text(&mut c, "ATM", 250.0, 370.0, 42.0, hex("#FFFFFF"), TextAlign::Left, None, true, false);
    fonts.draw_text(&mut c, "Vietcombank", 358.0, 382.0, 22.0, hex("#FFFFFF"), TextAlign::Left, Some(140.0), false, false);

    // Note [192, 432, 320, 80]
    c.fill_rect(192.0, 432.0, 320.0, 80.0, hex("#FFFFFF"));
    c.fill_rect(192.0, 432.0, 320.0, 8.0, vcb);
    fonts.draw_text(&mut c, "Rút tiền bằng mã QR", 352.0, 452.0, 20.0, hex("#16301F"), TextAlign::Center, Some(300.0), true, false);
    fonts.draw_text(&mut c, "Che tay khi nhập mã PIN", 352.0, 480.0, 16.0, hex("#16301F"), TextAlign::Center, Some(300.0), false, false);

    c.into_surface()
}

/// Exact Oracle `src/office/world/backdrops/street-ev.js` 1024×640 V-Green EV cabinet & LEZ sign atlas.
pub fn draw_ev_cabinet_atlas(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(1024, 640);
    let green = hex("#00A651");

    // Front [0, 0, 512, 640]
    c.fill_rect(0.0, 0.0, 512.0, 640.0, hex("#F4F6F5"));
    c.fill_rect(0.0, 0.0, 512.0, 96.0, green);
    c.fill_polygon(&[(54.0, 20.0), (32.0, 52.0), (49.0, 52.0), (43.0, 76.0), (66.0, 44.0), (50.0, 44.0)], hex("#FFFFFF"));
    fonts.draw_text(&mut c, "V-GREEN", 88.0, 22.0, 48.0, hex("#FFFFFF"), TextAlign::Left, Some(255.0), true, false);
    fonts.draw_text(&mut c, "TRẠM ĐỔI PIN", 352.0, 22.0, 20.0, hex("#FFFFFF"), TextAlign::Left, Some(150.0), true, false);
    fonts.draw_text(&mut c, "XE MÁY ĐIỆN", 352.0, 50.0, 20.0, hex("#FFFFFF"), TextAlign::Left, Some(150.0), true, false);

    for r in 0..5 {
        for (col_idx, lx) in [(r, 14.0_f32), (r + 5, 378.0_f32)] {
            let ly = 112.0 + (r as f32) * 102.0;
            c.fill_rect(lx, ly, 120.0, 92.0, hex("#DDE2E0"));
            c.stroke_rect(lx + 2.0, ly + 2.0, 116.0, 88.0, 3.0, hex("#B9C0BD"));
            c.fill_rect(lx + 92.0, ly + 34.0, 10.0, 26.0, hex("#C9CFCC"));
            let st = if col_idx % 5 == 3 { hex("#F2B544") } else { hex("#3DDC84") };
            c.fill_circle(lx + 22.0, ly + 22.0, 8.0, st);
            fonts.draw_text(&mut c, &format!("{:02}", col_idx + 1), lx + 38.0, ly + 14.0, 16.0, hex("#5E6A66"), TextAlign::Left, None, true, false);
        }
    }
    c.fill_rect(148.0, 112.0, 216.0, 190.0, hex("#1E2A33"));
    c.fill_rect(158.0, 122.0, 196.0, 170.0, hex("#E8F7EE"));
    c.fill_rect(158.0, 122.0, 196.0, 36.0, green);
    fonts.draw_text(&mut c, "ĐỔI PIN", 256.0, 130.0, 20.0, hex("#FFFFFF"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "9.000đ / lần", 256.0, 174.0, 20.0, hex("#16301F"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "1.5 kWh · 2 phút", 256.0, 208.0, 15.0, hex("#5E6A66"), TextAlign::Center, None, false, false);
    let _ = draw_qr_block(&mut c, 196.0, 324.0, 120.0, "https://vgreen.vn/swap/77");

    // Side [512, 0, 256, 640]
    c.fill_rect(512.0, 0.0, 256.0, 640.0, green);
    fonts.draw_text(&mut c, "V-GREEN", 640.0, 160.0, 46.0, hex("#FFFFFF"), TextAlign::Center, Some(230.0), true, false);
    fonts.draw_text(&mut c, "24/7", 640.0, 224.0, 42.0, hex("#D8FFE6"), TextAlign::Center, None, true, false);

    // LEZ sign [768, 0, 256, 320]
    c.fill_rect(768.0, 0.0, 256.0, 320.0, hex("#FFFFFF"));
    c.stroke_rect(774.0, 6.0, 244.0, 308.0, 8.0, hex("#00A651"));
    fonts.draw_text(&mut c, "VÙNG PHÁT THẢI", 896.0, 40.0, 22.0, hex("#007A3D"), TextAlign::Center, Some(220.0), true, false);
    fonts.draw_text(&mut c, "THẤP · HÀ NỘI", 896.0, 72.0, 22.0, hex("#007A3D"), TextAlign::Center, Some(220.0), true, false);
    fonts.draw_text(&mut c, "VÀNH ĐAI 1", 896.0, 146.0, 32.0, hex("#C8102E"), TextAlign::Center, Some(220.0), true, false);
    fonts.draw_text(&mut c, "Từ 01/07/2026", 896.0, 210.0, 20.0, hex("#1E2125"), TextAlign::Center, None, false, false);

    c.into_surface()
}

/// Exact Oracle `src/office/world/backdrops/street-photo.js` 1024×512 Korean photo studio atlas.
pub fn draw_photo_studio_atlas(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(1024, 512);
    let neon = hex("#FF4FA3");

    // Sign [0, 0, 1024, 192]
    c.fill_rect(0.0, 0.0, 1024.0, 192.0, hex("#2A1420"));
    fonts.draw_text(&mut c, "Chụp ảnh Hàn Quốc", 512.0, 30.0, 72.0, neon, TextAlign::Center, Some(840.0), true, false);
    fonts.draw_text(&mut c, "Chụp ảnh Hàn Quốc", 512.0, 28.0, 72.0, hex("#FFE6F3"), TextAlign::Center, Some(840.0), true, false);
    fonts.draw_text(&mut c, "· 4 KHUNG · SELF PHOTO STUDIO ·", 512.0, 128.0, 32.0, hex("#7FE3FF"), TextAlign::Center, Some(900.0), true, false);

    // Price board [0, 192, 384, 320]
    c.fill_rect(0.0, 192.0, 384.0, 320.0, hex("#FFF4F8"));
    c.stroke_rect(5.0, 197.0, 374.0, 310.0, 10.0, hex("#E8327A"));
    fonts.draw_text(&mut c, "BẢNG GIÁ", 192.0, 216.0, 34.0, hex("#E8327A"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "Dải 4 khung", 192.0, 268.0, 24.0, hex("#2B2F36"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "70.000đ", 192.0, 300.0, 38.0, hex("#E8327A"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "Khung lớn 6 ảnh", 192.0, 356.0, 24.0, hex("#2B2F36"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "100.000đ", 192.0, 388.0, 38.0, hex("#E8327A"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "Free phụ kiện · băng đô, kính, nón", 192.0, 444.0, 18.0, hex("#7A6A72"), TextAlign::Center, Some(356.0), false, false);
    fonts.draw_text(&mut c, "Mở cửa 9:00 – 22:00", 192.0, 472.0, 18.0, hex("#7A6A72"), TextAlign::Center, Some(356.0), false, false);

    // Sample strips wall [384, 192, 400, 200]
    c.fill_rect(384.0, 192.0, 400.0, 200.0, hex("#FBE3EC"));
    let cols = [hex("#FFD6E4"), hex("#D4F2E4"), hex("#E3DAFF"), hex("#FFF0B8")];
    for s in 0..8 {
        let sx = 394.0 + (s as f32) * 48.0;
        let sy = 204.0 + ((s % 2) as f32) * 6.0;
        c.fill_rect(sx, sy, 40.0, 170.0, hex("#FFFFFF"));
        for f in 0..4 {
            c.fill_rect(sx + 4.0, sy + 4.0 + (f as f32) * 40.0, 32.0, 34.0, cols[(s + f) % 4]);
            c.fill_circle(sx + 13.0, sy + 23.0 + (f as f32) * 40.0, 6.0, hex("#F2C9A8"));
            c.fill_circle(sx + 27.0, sy + 23.0 + (f as f32) * 40.0, 6.0, hex("#F2C9A8"));
        }
    }

    // Booth countdown screen [784, 192, 240, 160]
    c.fill_rect(784.0, 192.0, 240.0, 160.0, hex("#1E2125"));
    c.fill_rect(796.0, 204.0, 216.0, 136.0, hex("#FFD6E4"));
    fonts.draw_text(&mut c, "3", 904.0, 236.0, 64.0, hex("#E8327A"), TextAlign::Center, None, true, false);

    c.into_surface()
}

/// Exact Oracle `src/office/world/backdrops/street-pickle.js` 1024×512 pickleball atlas.
pub fn draw_pickleball_atlas(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(1024, 512);

    // Windscreen [0, 0, 1024, 128]
    c.fill_rect(0.0, 0.0, 1024.0, 128.0, hex("#1F4D3A"));
    for k in 0..2 {
        let cx = 256.0 + (k as f32) * 512.0;
        c.round_rect(cx - 170.0, 28.0, 36.0, 56.0, 12.0, hex("#F2C230"));
        c.fill_circle(cx - 122.0, 44.0, 13.0, hex("#D7E84A"));
        fonts.draw_text(&mut c, "PICKLEBALL", cx - 96.0, 40.0, 44.0, hex("#FFFFFF"), TextAlign::Left, Some(250.0), true, false);
    }

    // Gate board [0, 128, 512, 384]
    c.fill_rect(0.0, 128.0, 512.0, 384.0, hex("#FFFFFF"));
    c.fill_rect(0.0, 128.0, 512.0, 104.0, hex("#1F4D3A"));
    c.round_rect(42.0, 148.0, 38.0, 58.0, 12.0, hex("#F2C230"));
    c.fill_circle(92.0, 166.0, 13.0, hex("#D7E84A"));
    fonts.draw_text(&mut c, "SÂN PICKLEBALL", 118.0, 158.0, 44.0, hex("#FFFFFF"), TextAlign::Left, Some(380.0), true, false);
    c.fill_rect(0.0, 232.0, 512.0, 8.0, hex("#2D5E9E"));
    fonts.draw_text(&mut c, "Cho thuê sân theo giờ", 256.0, 258.0, 30.0, hex("#1E2125"), TextAlign::Center, Some(480.0), true, false);
    fonts.draw_text(&mut c, "200.000–300.000đ/giờ", 256.0, 308.0, 38.0, hex("#C8102E"), TextAlign::Center, Some(480.0), true, false);
    fonts.draw_text(&mut c, "sau 17h", 256.0, 356.0, 26.0, hex("#1E2125"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "Thuê vợt · bóng · nước suối", 256.0, 408.0, 22.0, hex("#1E2125"), TextAlign::Center, Some(480.0), false, false);
    fonts.draw_text(&mut c, "Mở cửa 5:00 – 22:00", 256.0, 446.0, 22.0, hex("#1E2125"), TextAlign::Center, Some(480.0), false, false);
    c.stroke_rect(4.0, 132.0, 504.0, 376.0, 8.0, hex("#1F4D3A"));

    // Ball skin [512, 128, 128, 128]
    c.fill_rect(512.0, 128.0, 128.0, 128.0, hex("#D9EA4E"));
    for i in 0..5 {
        for j in 0..3 {
            let bx = 525.0 + (i as f32) * 25.0 + ((j % 2) as f32) * 12.0;
            let by = 154.0 + (j as f32) * 38.0;
            c.fill_circle(bx, by, 6.0, hex("#5E6A1E"));
        }
    }

    c.into_surface()
}

struct ShopMeta<'a> {
    name: &'a str,
    sub: &'a str,
    logo: Option<&'a str>,
    face: &'a str,
    accent: &'a str,
    fg: &'a str,
    promo: (&'a str, &'a str, &'a str),
}

fn shop_meta(id: &str) -> ShopMeta<'static> {
    match id {
        "circlek" => ShopMeta { name: "Circle K", sub: "24/7 CONVENIENCE", logo: Some("circle-k.svg"), face: "#FFFFFF", accent: "#E21F26", fg: "#E21F26", promo: ("MÌ LY + TRÀ ĐÀO", "XÚC XÍCH NƯỚNG", "25K") },
        "gs25" => ShopMeta { name: "GS25", sub: "24/7 CONVENIENCE", logo: Some("gs25.svg"), face: "#FFFFFF", accent: "#0068B7", fg: "#0068B7", promo: ("TOKBOKKI CHẢ CÁ", "MÌ HÀN NẤU MÁY", "35K") },
        "seven" => ShopMeta { name: "7-Eleven", sub: "24/7 CONVENIENCE", logo: Some("7-eleven.svg"), face: "#FFFFFF", accent: "#008163", fg: "#008163", promo: ("ONIGIRI CƠM NẮM", "BÁNH BAO NÓNG", "17K") },
        "ministop" => ShopMeta { name: "Ministop", sub: "24/7 CONVENIENCE", logo: Some("ministop.svg"), face: "#FFFFFF", accent: "#003894", fg: "#003894", promo: ("KEM TƯƠI ỐC QUẾ", "GÀ RÁN GIÒN TAN", "10K") },
        "winmart" => ShopMeta { name: "WinMart+", sub: "TƯƠI NGON THƯỢNG HẠNG", logo: Some("winmart.png"), face: "#FFFFFF", accent: "#E21B2A", fg: "#E21B2A", promo: ("GIÁ TỐT MỖI NGÀY", "RAU CỦ TƯƠI MỚI", "-20%") },
        "bhx" => ShopMeta { name: "Bách Hóa Xanh", sub: "THỊT CÁ RAU CỦ TƯƠI", logo: Some("bach-hoa-xanh.png"), face: "#008C44", accent: "#FFE600", fg: "#FFFFFF", promo: ("RAU TƯƠI MỖI NGÀY", "THỊT CÁ TƯƠI SỐNG", "9K") },
        "mixue" => ShopMeta { name: "MIXUE", sub: "ICE CREAM & TEA", logo: None, face: "#E50914", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("KEM TƯƠI 10K", "TRÀ SỮA TRÂN CHÂU", "25K") },
        "highlands" => ShopMeta { name: "HIGHLANDS COFFEE", sub: "PHIN SỮA ĐÁ · FREEZE", logo: None, face: "#B22830", accent: "#F4E7D3", fg: "#FFFFFF", promo: ("PHIN SỮA ĐÁ", "FREEZE TRÀ XANH", "29K") },
        "tgdd" => ShopMeta { name: "THẾ GIỚI DI ĐỘNG", sub: "ĐIỆN THOẠI · LAPTOP", logo: None, face: "#FFD400", accent: "#111111", fg: "#111111", promo: ("TRẢ GÓP 0%", "THU CŨ ĐỔI MỚI", "HOT") },
        "katinat" => ShopMeta { name: "KATINAT", sub: "COFFEE & TEA HOUSE", logo: None, face: "#1C3247", accent: "#D4A66A", fg: "#F3E6D3", promo: ("TRÀ SỮA CHÔM CHÔM", "CÀ PHÊ SỮA ĐÁ", "45K") },
        "phuclong" => ShopMeta { name: "PHÚC LONG", sub: "COFFEE & TEA · SINCE 1968", logo: None, face: "#0C713D", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("TRÀ ĐÀO CAM SẢ", "TRÀ SỮA Ô LONG", "45K") },
        "longchau" => ShopMeta { name: "NHÀ THUỐC LONG CHÂU", sub: "FPT RETAIL · CHÍNH HÃNG", logo: None, face: "#1250DC", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("TƯ VẤN DƯỢC SĨ", "GIAO NHANH 1H", "24/7") },
        "dmx" => ShopMeta { name: "ĐIỆN MÁY XANH", sub: "TIVI · TỦ LẠNH · MÁY LẠNH", logo: None, face: "#0072CE", accent: "#FFF200", fg: "#FFF200", promo: ("BAO LẮP ĐẶT", "BẢO HÀNH CHÍNH HÃNG", "-30%") },
        "cong" => ShopMeta { name: "CỘNG CÀ PHÊ", sub: "CỐT DỪA CÀ PHÊ · HÀ NỘI", logo: None, face: "#3D4F35", accent: "#E8DFC5", fg: "#F5F0E1", promo: ("CỐT DỪA CÀ PHÊ", "BẠC XỈU ĐÁ", "45K") },
        "pho" => ShopMeta { name: "PHỞ BÒ", sub: "GIA TRUYỀN – TÁI CHÍN NẠM", logo: None, face: "#7A1F12", accent: "#FFE7A8", fg: "#FFE7A8", promo: ("PHỞ TÁI NẠM", "QUẨY NÓNG", "45K") },
        "bunbo" => ShopMeta { name: "BÚN BÒ HUẾ", sub: "GIÒ HEO – CHẢ CUA", logo: None, face: "#B3121E", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("BÚN BÒ ĐẶC BIỆT", "CHẢ CUA HUẾ", "45K") },
        "miquang" => ShopMeta { name: "MÌ QUẢNG", sub: "ĐẶC SẢN QUẢNG NAM – ĐÀ NẴNG", logo: None, face: "#B3121E", accent: "#FFD400", fg: "#FFD400", promo: ("MÌ QUẢNG TÔM THỊT", "BÁNH TRÁNG NƯỚNG", "40K") },
        "haisan" => ShopMeta { name: "HẢI SẢN", sub: "TƯƠI SỐNG MỖI NGÀY", logo: None, face: "#0B4F8A", accent: "#FFE45C", fg: "#FFE45C", promo: ("TÔM HÙM · GHẸ XANH", "ỐC HƯƠNG NƯỚNG", "TƯƠI") },
        "comhen" => ShopMeta { name: "CƠM HẾN", sub: "BÚN HẾN – HẾN XÚC BÁNH TRÁNG", logo: None, face: "#1F6F3B", accent: "#FFF2A8", fg: "#FFF2A8", promo: ("CƠM HẾN HUẾ", "CHÈ ĐẬU VÁN", "25K") },
        "bunchaca" => ShopMeta { name: "BÚN CHẢ CÁ", sub: "ĐẶC SẢN NHA TRANG", logo: None, face: "#0B4F8A", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("BÚN CHẢ CÁ SỨA", "NEM NƯỚNG", "40K") },
        "banhcan" => ShopMeta { name: "BÁNH CĂN", sub: "ĐÀ LẠT", logo: None, face: "#7A3B12", accent: "#FFE7A8", fg: "#FFE7A8", promo: ("BÁNH CĂN TRỨNG", "XÍU MẠI NÓNG", "35K") },
        "suadau" => ShopMeta { name: "SỮA ĐẬU NÀNH", sub: "NÓNG – BÁNH NGỌT", logo: None, face: "#2F6B3A", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("SỮA ĐẬU NÀNH NÓNG", "BÁNH SỪNG TRÂU", "15K") },
        "banhdacua" => ShopMeta { name: "BÁNH ĐA CUA", sub: "ĐẶC SẢN HẢI PHÒNG", logo: None, face: "#A0281C", accent: "#FFE45C", fg: "#FFE45C", promo: ("BÁNH ĐA CUA CHẢ LÁ LỐT", "BÁNH MÌ CAY", "40K") },
        "hutieu" => ShopMeta { name: "HỦ TIẾU", sub: "CÁI RĂNG – CẦN THƠ", logo: None, face: "#B3121E", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("HỦ TIẾU NAM VANG", "HỦ TIẾU KHÔ", "40K") },
        "laumam" => ShopMeta { name: "LẨU MẮM", sub: "MIỀN TÂY", logo: None, face: "#5C2E7E", accent: "#FFE45C", fg: "#FFE45C", promo: ("LẨU MẮM CÁ LINH", "BÔNG ĐIÊN ĐIỂN", "199K") },
        "buncha" => ShopMeta { name: "BÚN CHẢ", sub: "HÀ NỘI – NEM CUA BỂ", logo: None, face: "#2B4A1E", accent: "#FFE45C", fg: "#FFE45C", promo: ("BÚN CHẢ NƯỚNG THAN", "NEM CUA BỂ", "45K") },
        _ => ShopMeta { name: "BÁNH XÈO", sub: "NÓNG GIÒN – RAU RỪNG", logo: None, face: "#B8620F", accent: "#FFFFFF", fg: "#FFFFFF", promo: ("BÁNH XÈO TÔM MỰC", "NEM LỤI", "35K") },
    }
}

/// Exact Oracle `src/office/world/backdrops/shops.js` 1024×512 shop cell atlas (fascia + posters + menu + QR).
pub fn draw_shop_cell(
    chain_id: &str,
    brands: &mut BrandRegistry,
    fonts: &mut FontEngine,
) -> RgbaSurface {
    let m = shop_meta(chain_id);
    let mut c = Canvas2d::new(1024, 512);
    c.fill(hex("#F6F5F1"));

    // Sign fascia [0, 0, 1024, 140]
    c.fill_rect(0.0, 0.0, 1024.0, 140.0, hex(m.face));
    c.fill_rect(0.0, 124.0, 1024.0, 16.0, hex(m.accent));

    if let Some(logo_file) = m.logo
        && let Ok(logo) = brands.load_brand_rgba(logo_file, 260, 100)
    {
        let lx = 36;
        let ly = ((120 - logo.height as i32) / 2).max(6);
        c.blit_rgba(&logo, lx, ly, logo.width, logo.height);
        fonts.draw_text(&mut c, m.name, 330.0, 24.0, 52.0, hex(m.fg), TextAlign::Left, Some(650.0), true, false);
        fonts.draw_text(&mut c, m.sub, 330.0, 84.0, 22.0, hex(m.fg), TextAlign::Left, Some(650.0), false, false);
    } else {
        fonts.draw_text(&mut c, m.name, 512.0, 22.0, 58.0, hex(m.fg), TextAlign::Center, Some(960.0), true, false);
        fonts.draw_text(&mut c, m.sub, 512.0, 86.0, 22.0, hex(m.fg), TextAlign::Center, Some(960.0), false, false);
    }

    let accent_col = hex(m.accent);
    let accent_luma = (accent_col[0] as u32 * 299 + accent_col[1] as u32 * 587 + accent_col[2] as u32 * 114) / 1000;
    let on_accent = if accent_luma > 165 { hex("#221814") } else { hex("#FFFFFF") };
    let price_on_accent = if accent_luma > 165 { hex("#B3121E") } else { hex("#FFE600") };

    // Promo Poster A [0, 140, 200, 290] & B [200, 140, 200, 290]
    c.fill_rect(8.0, 148.0, 184.0, 274.0, accent_col);
    fonts.draw_text(&mut c, m.promo.0, 100.0, 196.0, 24.0, on_accent, TextAlign::Center, Some(168.0), true, false);
    fonts.draw_text(&mut c, m.promo.2, 100.0, 278.0, 52.0, price_on_accent, TextAlign::Center, Some(168.0), true, false);

    c.fill_rect(208.0, 148.0, 184.0, 274.0, hex("#1E242E"));
    fonts.draw_text(&mut c, m.promo.1, 300.0, 210.0, 22.0, hex("#FFFFFF"), TextAlign::Center, Some(168.0), true, false);
    fonts.draw_text(&mut c, "MỞ CỬA", 300.0, 296.0, 26.0, hex("#3DDC84"), TextAlign::Center, Some(168.0), true, false);

    // Menu board [400, 140, 420, 200]
    c.fill_rect(408.0, 148.0, 404.0, 184.0, hex("#22262E"));
    c.fill_rect(408.0, 148.0, 404.0, 38.0, accent_col);
    fonts.draw_text(&mut c, "THỰC ĐƠN · MENU", 610.0, 156.0, 20.0, on_accent, TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, m.promo.0, 428.0, 206.0, 20.0, hex("#F0F2F5"), TextAlign::Left, Some(280.0), false, false);
    fonts.draw_text(&mut c, m.promo.2, 792.0, 206.0, 20.0, hex("#FFD400"), TextAlign::Right, None, true, false);
    fonts.draw_text(&mut c, m.promo.1, 428.0, 246.0, 20.0, hex("#F0F2F5"), TextAlign::Left, Some(280.0), false, false);
    fonts.draw_text(&mut c, "HOT", 792.0, 246.0, 20.0, hex("#3DDC84"), TextAlign::Right, None, true, false);

    // Door & QR stand [820, 140, 204, 290]
    c.round_rect(836.0, 156.0, 172.0, 172.0, 16.0, hex("#FFFFFF"));
    c.stroke_round_rect(836.0, 156.0, 172.0, 172.0, 16.0, 4.0, accent_col);
    fonts.draw_text(&mut c, "OPEN", 922.0, 202.0, 34.0, if accent_luma > 165 { hex("#7A1F12") } else { accent_col }, TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "PUSH · ĐẨY", 922.0, 256.0, 18.0, hex("#4A515B"), TextAlign::Center, None, false, false);
    let _ = draw_qr_block(&mut c, 830.0, 346.0, 84.0, &format!("https://agents-station.local/shop/{chain_id}"));

    // Back wall accent band [0, 432, 1024, 80]
    c.fill_rect(0.0, 432.0, 1024.0, 80.0, accent_col);
    fonts.draw_text(&mut c, &format!("{} · {}", m.name, m.sub), 512.0, 456.0, 28.0, on_accent, TextAlign::Center, Some(960.0), true, false);

    c.into_surface()
}

/// Simple deterministic LCG matching Oracle procedural texture seeding.
fn lcg_next(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*seed >> 8) as f32) / ((1u32 << 24) as f32)
}

/// Exact Oracle `city.js:22-28` grainy 256×256 repeating asphalt texture (multiplied by day/night tint).
pub fn draw_asphalt_tile() -> RgbaSurface {
    let mut c = Canvas2d::new(256, 256);
    c.fill(hex("#E4E4E4"));
    let mut seed = 2026_1026u32;
    for _ in 0..4_500 {
        let v = 180 + (lcg_next(&mut seed) * 75.0) as u8;
        let x = lcg_next(&mut seed) * 255.0;
        let y = lcg_next(&mut seed) * 255.0;
        let s = 1.0 + lcg_next(&mut seed) * 1.5;
        c.fill_rect(x, y, s, s, [v, v, v, 255]);
    }
    for _ in 0..5 {
        let cx = lcg_next(&mut seed) * 256.0;
        let cy = lcg_next(&mut seed) * 256.0;
        let rx = 18.0 + lcg_next(&mut seed) * 36.0;
        let ry = 10.0 + lcg_next(&mut seed) * 22.0;
        c.fill_ellipse(cx, cy, rx, ry, [0, 0, 0, 18]);
    }
    for _ in 0..4 {
        let mut x = lcg_next(&mut seed) * 256.0;
        let mut y = lcg_next(&mut seed) * 256.0;
        for _ in 0..6 {
            let nx = x + (lcg_next(&mut seed) - 0.5) * 30.0;
            let ny = y + (lcg_next(&mut seed) - 0.5) * 30.0;
            c.stroke_line(x, y, nx, ny, 1.0, [0, 0, 0, 46]);
            x = nx;
            y = ny;
        }
    }
    c.into_surface()
}

struct FacadeStyle {
    win: [f32; 4],
    glass_top: &'static str,
    glass_bot: &'static str,
    wall: &'static str,
    frame: Option<&'static str>,
    strips: bool,
    brick: bool,
    balcony: bool,
}

const FACADE_STYLES: [FacadeStyle; 4] = [
    FacadeStyle {
        win: [0.05, 0.30, 0.95, 0.95],
        glass_top: "#B8D3E0",
        glass_bot: "#2A4458",
        wall: "#DCE3E8",
        frame: None,
        strips: true,
        brick: false,
        balcony: false,
    },
    FacadeStyle {
        win: [0.28, 0.33, 0.72, 0.84],
        glass_top: "#9EB3C4",
        glass_bot: "#232D38",
        wall: "#F4F1EB",
        frame: Some("#FFFFFF"),
        strips: false,
        brick: false,
        balcony: false,
    },
    FacadeStyle {
        win: [0.25, 0.30, 0.75, 0.80],
        glass_top: "#94A9BA",
        glass_bot: "#1F252C",
        wall: "#FFFFFF",
        frame: Some("#E9E2D6"),
        strips: false,
        brick: true,
        balcony: false,
    },
    FacadeStyle {
        win: [0.12, 0.30, 0.88, 0.88],
        glass_top: "#AFC6D6",
        glass_bot: "#263441",
        wall: "#EFEFEF",
        frame: None,
        strips: false,
        brick: false,
        balcony: true,
    },
];

/// Exact Oracle `city.js:41-60` 64×64 one-floor, one-bay daytime facade module.
pub fn draw_facade_day_tile(style: usize) -> RgbaSurface {
    let st = &FACADE_STYLES[style % FACADE_STYLES.len()];
    let mut c = Canvas2d::new(64, 64);
    c.fill(hex(st.wall));
    let [x0, y0, x1, y1] = st.win;
    let fx = |f: f32| f * 64.0;
    let fy = |f: f32| (1.0 - f) * 64.0;

    if st.brick {
        let mut y = 2.0;
        while y < 64.0 {
            c.fill_rect(0.0, y, 64.0, 1.0, [0, 0, 0, 20]);
            y += 5.0;
        }
    }
    if st.strips {
        c.fill_rect(0.0, fy(0.30), 64.0, fy(0.0) - fy(0.30), [0, 0, 0, 46]);
    }
    let wy = fy(y1);
    let wh = fy(y0) - fy(y1);
    let wx = fx(x0);
    let ww = fx(x1) - fx(x0);
    c.linear_gradient_rect(
        wx,
        wy,
        ww,
        wh,
        (0.0, wy),
        (0.0, wy + wh),
        &[
            (0.0, hex(st.glass_top)),
            (0.45, hex(st.glass_bot)),
            (1.0, hex(st.glass_bot)),
        ],
    );
    if let Some(frame_hex) = st.frame {
        let fc = hex(frame_hex);
        c.stroke_rect(wx, wy, ww, wh, 2.0, fc);
        c.fill_rect(wx - 2.0, fy(y0), ww + 4.0, 3.0, fc);
    }
    if st.strips {
        c.fill_rect(31.0, wy, 2.0, wh, [255, 255, 255, 90]);
        c.fill_rect(0.0, wy, 2.0, wh, [255, 255, 255, 90]);
    }
    if st.balcony {
        c.fill_rect(0.0, fy(0.30), 64.0, 4.0, [0, 0, 0, 72]);
        c.fill_rect(0.0, fy(0.30) - 2.0, 64.0, 2.0, [255, 255, 255, 140]);
        c.fill_rect(0.0, fy(0.50), 64.0, 2.0, [40, 46, 54, 140]);
    }
    c.into_surface()
}

/// Compact Oracle `city.js:62-80` 256×512 (16×32 bay) night-lit building window emissive atlas.
pub fn draw_facade_night_atlas(style: usize) -> RgbaSurface {
    let st = &FACADE_STYLES[style % FACADE_STYLES.len()];
    let mut c = Canvas2d::new(256, 512);
    c.fill(hex("#000000"));
    let [x0, y0, x1, y1] = st.win;
    let mut seed = 91_731u32.wrapping_add((style as u32).wrapping_mul(17_389));
    for r in 0..32u32 {
        let kind = lcg_next(&mut seed);
        let p = if kind < 0.55 {
            0.08
        } else if kind < 0.86 {
            0.26
        } else {
            0.55
        };
        let mut run = 0u32;
        for col in 0..16u32 {
            let mut on = lcg_next(&mut seed) < p;
            if st.strips {
                if run > 0 {
                    on = true;
                    run -= 1;
                } else if on {
                    run = 1 + (lcg_next(&mut seed) * 4.0) as u32;
                }
            }
            if !on {
                continue;
            }
            let k = lcg_next(&mut seed);
            let base = if k < 0.68 {
                hex("#FFD39A")
            } else if k < 0.84 {
                hex("#FFF1DA")
            } else if k < 0.95 {
                hex("#BCD6FF")
            } else {
                hex("#7FA8FF")
            };
            let alpha = if k >= 0.95 {
                0.45
            } else {
                0.40 + lcg_next(&mut seed) * 0.50
            };
            let lit = [
                (base[0] as f32 * alpha) as u8,
                (base[1] as f32 * alpha) as u8,
                (base[2] as f32 * alpha) as u8,
                255,
            ];
            let cx = (col * 16) as f32;
            let cy = ((31 - r) * 16) as f32;
            c.fill_rect(
                cx + x0 * 16.0,
                cy + (1.0 - y1) * 16.0,
                (x1 - x0) * 16.0,
                (y1 - y0) * 16.0,
                lit,
            );
        }
    }
    c.into_surface()
}

/// Exact Oracle `city.js:88-99` 512×64 strip of 8 glowing ground-floor neighborhood shop fronts.
pub fn draw_generic_shop_strip() -> RgbaSurface {
    let mut c = Canvas2d::new(512, 64);
    let signs = [
        "#FF5D73", "#3DDC84", "#F2B544", "#4FC3F7", "#B388FF", "#FF8A3D", "#E0E0E0", "#64FFDA",
    ];
    for (i, sign_hex) in signs.iter().enumerate() {
        let x = (i * 64) as f32;
        c.fill_rect(x, 0.0, 64.0, 64.0, hex("#1C2026"));
        c.linear_gradient_rect(
            x + 4.0,
            23.0,
            56.0,
            41.0,
            (0.0, 22.0),
            (0.0, 64.0),
            &[(0.0, hex("#FFE7B8")), (1.0, hex("#C98E4A"))],
        );
        for k in 1..4 {
            c.fill_rect(x + 4.0 + k as f32 * 14.0, 23.0, 1.5, 41.0, [0, 0, 0, 90]);
        }
        c.fill_rect(x + 7.0, 5.0, 50.0, 12.0, hex(sign_hex));
        c.fill_rect(x + 13.0, 9.5, 38.0, 3.0, [255, 255, 255, 215]);
    }
    c.into_surface()
}

/// Soft warm amber 64×64 radial streetlamp ground pool texture (`city.js:305`).
pub fn draw_lamp_pool_tex() -> RgbaSurface {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let dx = (x as f32 + 0.5 - 32.0) / 31.5;
            let dy = (y as f32 + 0.5 - 32.0) / 31.5;
            let r = (dx * dx + dy * dy).sqrt().min(1.0);
            let falloff = (1.0 - r).powf(2.2);
            let idx = (y * 64 + x) * 4;
            rgba[idx] = 255;
            rgba[idx + 1] = 190;
            rgba[idx + 2] = 110;
            rgba[idx + 3] = (falloff * 115.0).round() as u8;
        }
    }
    RgbaSurface {
        width: 64,
        height: 64,
        rgba,
    }
}

/// Exact Oracle `eat-com.js:37-107` 1024×512 *Quán cơm bình dân* + Friday *Bún đậu mắm tôm* atlas:
/// - Sign `[0, 0, 512, 160]`
/// - Weekday Menu `[512, 0, 256, 256]`
/// - Friday Bún Đậu Board `[768, 0, 256, 256]`
/// - 8 Dish Trays `[k*128, 256, 128, 96]` for `k = 0..8`
pub fn draw_com_binh_dan_atlas(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(1024, 512);
    c.fill([0, 0, 0, 0]);

    // Sign [0, 0, 512, 160]
    c.fill_rect(0.0, 0.0, 512.0, 160.0, hex("#F7D23E"));
    c.stroke_rect(6.0, 6.0, 500.0, 148.0, 10.0, hex("#C8102E"));
    fonts.draw_text(&mut c, "CƠM BÌNH DÂN", 256.0, 32.0, 54.0, hex("#C8102E"), TextAlign::Center, Some(470.0), true, false);
    fonts.draw_text(&mut c, "Cơm văn phòng · 35K · Trà đá free", 256.0, 108.0, 24.0, hex("#1E3A8A"), TextAlign::Center, Some(470.0), true, false);

    // Weekday menu [512, 0, 256, 256]
    c.fill_rect(512.0, 0.0, 256.0, 256.0, hex("#F7F7F2"));
    c.stroke_rect(516.0, 4.0, 248.0, 248.0, 8.0, hex("#2B2F36"));
    fonts.draw_text(&mut c, "HÔM NAY CÓ", 640.0, 18.0, 22.0, hex("#C8102E"), TextAlign::Center, Some(228.0), true, false);
    let dishes = [
        ("• Thịt kho trứng", "#1E3A8A"),
        ("• Cá kho tộ", "#1E3A8A"),
        ("• Gà chiên mắm", "#1E3A8A"),
        ("• Khổ qua dồn thịt", "#1E3A8A"),
        ("• Canh chua cá", "#1E8A3A"),
        ("• Rau muống xào", "#1E8A3A"),
    ];
    for (i, (label, col)) in dishes.iter().enumerate() {
        fonts.draw_text(&mut c, label, 528.0, 54.0 + i as f32 * 27.0, 17.0, hex(col), TextAlign::Left, Some(224.0), true, false);
    }
    fonts.draw_text(&mut c, "35K / phần", 640.0, 220.0, 24.0, hex("#C8102E"), TextAlign::Center, Some(228.0), true, false);

    // Friday Bún đậu board [768, 0, 256, 256]
    c.fill_rect(768.0, 0.0, 256.0, 256.0, hex("#F7F7F2"));
    c.stroke_rect(772.0, 4.0, 248.0, 248.0, 8.0, hex("#2B2F36"));
    fonts.draw_text(&mut c, "THỨ 6", 896.0, 18.0, 28.0, hex("#7A3A8A"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "BÚN ĐẬU", 896.0, 64.0, 28.0, hex("#C8102E"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "MẮM TÔM", 896.0, 100.0, 28.0, hex("#C8102E"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "Mẹt thường 45K", 896.0, 152.0, 18.0, hex("#1E3A8A"), TextAlign::Center, Some(228.0), true, false);
    fonts.draw_text(&mut c, "Mẹt đầy đủ 65K", 896.0, 180.0, 18.0, hex("#1E3A8A"), TextAlign::Center, Some(228.0), true, false);
    fonts.draw_text(&mut c, "chả cốm · thịt luộc · đậu rán", 896.0, 214.0, 13.0, hex("#1E3A8A"), TextAlign::Center, Some(228.0), false, false);

    // 8 Dish trays [k*128, 256, 128, 96]
    let tray_bg = [
        "#783C16", "#642E0E", "#356B24", "#C94A24", "#A46418", "#E0B43A", "#DC7828", "#A89868",
    ];
    for k in 0..8usize {
        let x = (k * 128) as f32;
        let y = 256.0;
        c.fill_rect(x, y, 128.0, 96.0, hex(tray_bg[k]));
        let mut s = (k as u32 + 3) * 9301 + 49297;
        match k {
            0 => {
                // Thịt kho trứng
                for i in 0..6 {
                    let px = x + 16.0 + lcg_next(&mut s) * 92.0;
                    let py = y + 14.0 + lcg_next(&mut s) * 66.0;
                    c.fill_rect(px - 10.0, py - 7.0, 20.0, 14.0, hex("#6B3416"));
                    c.fill_rect(px - 10.0, py - 2.0, 20.0, 4.0, hex("#E8C9A0"));
                    if i < 4 {
                        c.fill_ellipse(x + 22.0 + i as f32 * 26.0, y + 48.0, 11.0, 8.5, hex("#D9B98A"));
                    }
                }
            }
            1 => {
                // Cá kho tộ
                for i in 0..5 {
                    let px = x + 22.0 + (i % 3) as f32 * 38.0;
                    let py = y + 26.0 + (i / 3) as f32 * 40.0;
                    c.fill_ellipse(px, py, 16.0, 12.0, hex("#4A2A1A"));
                    c.fill_ellipse(px, py, 12.0, 8.5, hex("#B88A5A"));
                }
                for _ in 0..14 {
                    c.fill_rect(x + lcg_next(&mut s) * 120.0, y + lcg_next(&mut s) * 90.0, 5.0, 3.0, hex("#5FA83A"));
                }
            }
            2 => {
                // Rau muống xào tỏi
                for i in 0..24 {
                    let px = x + lcg_next(&mut s) * 105.0;
                    let py = y + lcg_next(&mut s) * 80.0;
                    let col = if i % 3 == 0 { hex("#9BCB5A") } else { hex("#4F9A34") };
                    c.stroke_line(px, py, px + 22.0, py + 10.0, 3.0, col);
                }
                for _ in 0..14 {
                    c.fill_rect(x + lcg_next(&mut s) * 120.0, y + lcg_next(&mut s) * 90.0, 4.0, 4.0, hex("#F2E2A0"));
                }
            }
            3 => {
                // Đậu hũ sốt cà chua
                for i in 0..8 {
                    let px = x + 18.0 + (i % 4) as f32 * 28.0;
                    let py = y + 20.0 + (i / 4) as f32 * 34.0;
                    c.fill_rect(px - 10.0, py - 8.0, 20.0, 16.0, hex("#E9C46A"));
                    c.fill_rect(px - 10.0, py + 2.0, 20.0, 6.0, [200, 60, 20, 120]);
                }
            }
            4 => {
                // Gà chiên nước mắm
                for _ in 0..7 {
                    let px = x + 18.0 + lcg_next(&mut s) * 92.0;
                    let py = y + 18.0 + lcg_next(&mut s) * 60.0;
                    c.fill_ellipse(px, py, 14.0, 10.0, hex("#C98A2A"));
                    c.fill_ellipse(px - 3.0, py - 2.0, 7.0, 4.5, hex("#E8B04A"));
                }
            }
            5 => {
                // Trứng chiên
                for i in 0..4 {
                    let px = x + 8.0 + (i % 2) as f32 * 56.0;
                    let py = y + 8.0 + (i / 2) as f32 * 42.0;
                    c.round_rect(px, py, 50.0, 36.0, 7.0, hex("#F2CF4A"));
                    c.stroke_round_rect(px, py, 50.0, 36.0, 7.0, 2.5, hex("#B87A1A"));
                    c.fill_rect(px + 14.0, py + 14.0, 5.0, 3.0, hex("#5FA83A"));
                }
            }
            6 => {
                // Canh chua
                c.fill_rect(x, y, 128.0, 96.0, hex("#B9C0C8"));
                c.fill_circle(x + 64.0, y + 48.0, 44.0, hex("#E07A2A"));
                for _ in 0..6 {
                    let px = x + 36.0 + lcg_next(&mut s) * 56.0;
                    let py = y + 20.0 + lcg_next(&mut s) * 56.0;
                    c.fill_rect(px, py, 9.0, 7.0, hex("#F2D04A"));
                    c.fill_circle(px + 6.0, py + 6.0, 5.0, hex("#D8262E"));
                }
            }
            _ => {
                // Khổ qua dồn thịt
                for i in 0..6 {
                    let px = x + 22.0 + (i % 3) as f32 * 40.0;
                    let py = y + 28.0 + (i / 3) as f32 * 40.0;
                    c.fill_circle(px, py, 16.0, hex("#6FA03A"));
                    c.fill_circle(px, py, 11.0, hex("#A8C878"));
                    c.fill_circle(px, py, 8.0, hex("#C89A7A"));
                }
            }
        }
        c.fill_rect(x, y, 128.0, 28.0, [255, 255, 255, 18]);
    }

    c.into_surface()
}

/// Shared 512×512 packed street-life signs atlas (1 MB GPU vs 8 separate textures):
/// - `XÔI · BÁNH CUỐN` `[0, 0, 256, 128]`
/// - `VÁ XE · SỬA XE` `[256, 0, 256, 128]`
/// - `QUÁN NHẬU · BIA HƠI` `[0, 128, 256, 128]`
/// - `LỄ VU QUY · 囍` `[256, 128, 256, 128]`
/// - `CSGT · KIỂM TRA` `[0, 256, 256, 128]`
/// - `CẤP CỨU 115` `[256, 256, 256, 128]`
/// - `CỨU HỎA 114` `[0, 384, 256, 128]`
/// - `TRÀ ĐÁ · CỜ TƯỚNG` `[256, 384, 256, 128]`
pub fn draw_street_signs_atlas(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 512);
    c.fill([0, 0, 0, 0]);

    // [0, 0, 256, 128] Xôi · Bánh cuốn
    c.fill_rect(0.0, 0.0, 256.0, 128.0, hex("#E9D7B0"));
    c.stroke_rect(4.0, 4.0, 248.0, 120.0, 4.0, hex("#B5122B"));
    fonts.draw_text(&mut c, "XÔI", 128.0, 20.0, 36.0, hex("#B5122B"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "BÁNH CUỐN", 128.0, 68.0, 34.0, hex("#B5122B"), TextAlign::Center, Some(236.0), true, false);

    // [256, 0, 256, 128] Vá xe
    c.fill_rect(256.0, 0.0, 256.0, 128.0, hex("#F7D23E"));
    c.stroke_rect(260.0, 4.0, 248.0, 120.0, 6.0, hex("#1E2125"));
    fonts.draw_text(&mut c, "VÁ XE", 384.0, 22.0, 44.0, hex("#C8102E"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "BƠM HƠI · SỬA XE", 384.0, 78.0, 20.0, hex("#1E2125"), TextAlign::Center, Some(236.0), true, false);

    // [0, 128, 256, 128] Quán nhậu
    c.fill_rect(0.0, 128.0, 256.0, 128.0, hex("#1F4FA0"));
    c.stroke_rect(4.0, 132.0, 248.0, 120.0, 5.0, hex("#FFD400"));
    fonts.draw_text(&mut c, "QUÁN NHẬU", 128.0, 148.0, 34.0, hex("#FFD400"), TextAlign::Center, Some(236.0), true, false);
    fonts.draw_text(&mut c, "BIA HƠI · ỐC · LẨU", 128.0, 198.0, 20.0, hex("#FFFFFF"), TextAlign::Center, Some(236.0), true, false);

    // [256, 128, 256, 128] Lễ Vu Quy
    c.fill_rect(256.0, 128.0, 256.0, 128.0, hex("#C8102E"));
    c.stroke_rect(262.0, 134.0, 244.0, 116.0, 5.0, hex("#F7D23E"));
    fonts.draw_text(&mut c, "LỄ VU QUY", 384.0, 152.0, 34.0, hex("#FFF2A8"), TextAlign::Center, Some(232.0), true, false);
    fonts.draw_text(&mut c, "TRĂM NĂM HẠNH PHÚC", 384.0, 202.0, 18.0, hex("#FFFFFF"), TextAlign::Center, Some(232.0), true, false);

    // [0, 256, 256, 128] CSGT
    c.fill_rect(0.0, 256.0, 256.0, 128.0, hex("#1C4FA6"));
    c.stroke_rect(6.0, 262.0, 244.0, 116.0, 6.0, hex("#FFFFFF"));
    fonts.draw_text(&mut c, "C.S.G.T", 128.0, 274.0, 38.0, hex("#FFD400"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "CHỐT KIỂM TRA", 128.0, 326.0, 22.0, hex("#FFFFFF"), TextAlign::Center, Some(230.0), true, false);

    // [256, 256, 256, 128] Cấp cứu 115
    c.fill_rect(256.0, 256.0, 256.0, 128.0, hex("#FFFFFF"));
    c.fill_rect(256.0, 256.0, 256.0, 16.0, hex("#C8102E"));
    c.fill_rect(256.0, 368.0, 256.0, 16.0, hex("#C8102E"));
    fonts.draw_text(&mut c, "CẤP CỨU", 384.0, 278.0, 26.0, hex("#C8102E"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "115", 384.0, 310.0, 48.0, hex("#C8102E"), TextAlign::Center, None, true, false);

    // [0, 384, 256, 128] Cứu hỏa 114
    c.fill_rect(0.0, 384.0, 256.0, 128.0, hex("#C8102E"));
    fonts.draw_text(&mut c, "CỨU HỎA", 128.0, 404.0, 26.0, hex("#FFFFFF"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "114", 128.0, 438.0, 48.0, hex("#FFD400"), TextAlign::Center, None, true, false);

    // [256, 384, 256, 128] Trà đá · Cờ tướng
    c.fill_rect(256.0, 384.0, 256.0, 128.0, hex("#2A7A42"));
    c.stroke_rect(262.0, 390.0, 244.0, 116.0, 4.0, hex("#FFF2A8"));
    fonts.draw_text(&mut c, "TRÀ ĐÁ · HƯỚNG DƯƠNG", 384.0, 412.0, 18.0, hex("#FFF2A8"), TextAlign::Center, Some(232.0), true, false);
    fonts.draw_text(&mut c, "5K · VỈA HÈ", 384.0, 452.0, 26.0, hex("#FFFFFF"), TextAlign::Center, Some(232.0), true, false);

    c.into_surface()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn street_and_shop_surfaces_render_all_chains_and_signs() {
        let mut fonts = FontEngine::from_assets_dir(&assets());
        let mut brands = BrandRegistry::new(assets());

        for id in ["circlek", "gs25", "seven", "ministop", "winmart", "bhx", "highlands", "pho", "bunbo", "miquang"] {
            let surf = draw_shop_cell(id, &mut brands, &mut fonts);
            assert_eq!((surf.width, surf.height), (1024, 512));
            assert!(surf.non_background_ratio(hex("#F6F5F1")) > 0.35, "shop {id}");
        }
        assert_eq!(draw_atm_atlas(&mut fonts).width, 512);
        assert_eq!(draw_ev_cabinet_atlas(&mut fonts).width, 1024);
        assert_eq!(draw_photo_studio_atlas(&mut fonts).width, 1024);
        assert_eq!(draw_pickleball_atlas(&mut fonts).width, 1024);
        for col in [SignalColor::Red, SignalColor::Green, SignalColor::Amber] {
            let led = draw_traffic_countdown(18, col);
            assert_eq!((led.width, led.height), (64, 40));
            assert!(led.non_background_ratio(hex("#050505")) > 0.1);
        }
    }

    #[test]
    fn real_qr_rgba_encodes_iso18004_finder_patterns_and_distinguishes_urls() {
        let (a_rgba, aw, ah) =
            render_qr_rgba("http://192.168.1.24:4310/phone", 200).expect("encode lan url");
        let (b_rgba, bw, bh) =
            render_qr_rgba("https://demo.trycloudflare.com/phone", 200).expect("encode tunnel url");

        assert_eq!(aw, ah);
        assert_eq!(bw, bh);
        assert_eq!(a_rgba.len(), (aw * ah * 4) as usize);
        assert_eq!(b_rgba.len(), (bw * bh * 4) as usize);
        assert_ne!(
            a_rgba, b_rgba,
            "distinct URLs must produce distinct QR pixel matrices"
        );

        // Verify quiet zone (top-left pixel is pure white) and dark modules exist
        assert_eq!(&a_rgba[0..4], &[255, 255, 255, 255], "quiet zone must be white");
        let dark_pixels = a_rgba.chunks_exact(4).filter(|p| p[0] < 64).count();
        assert!(
            dark_pixels > (aw as usize * ah as usize) / 8,
            "QR must contain substantial dark modules, got {dark_pixels}"
        );

        // Verify ISO-18004 finder pattern: top-left 7x7 finder box after 4-module quiet zone
        let code = qrcode::QrCode::new(b"http://192.168.1.24:4310/phone").unwrap();
        let total_mods = code.width() + 8; // 4-module quiet zone on both sides
        let scale = (aw as usize) / total_mods;
        assert!(scale >= 1, "expected integer module scale >= 1");
        let is_dark = |mx: usize, my: usize| -> bool {
            let px = mx * scale + scale / 2;
            let py = my * scale + scale / 2;
            let idx = (py * aw as usize + px) * 4;
            a_rgba[idx] < 64
        };
        // Top-left finder corners (module 4..11) + white separator + dark center (6..9)
        assert!(is_dark(4, 4), "finder outer border must be dark");
        assert!(!is_dark(5, 5), "finder inner white ring must be light");
        assert!(is_dark(6, 6), "finder 3x3 core must be dark");
        // Top-right and bottom-left finder cores
        let tr = 4 + code.width() - 7;
        assert!(is_dark(tr + 2, 6), "top-right finder core must be dark");
        assert!(is_dark(6, tr + 2), "bottom-left finder core must be dark");
    }

    #[test]
    fn city_facade_asphalt_com_and_street_signs_atlases_render_valid_pixels() {
        let mut fonts = FontEngine::from_assets_dir(&assets());

        let asphalt = draw_asphalt_tile();
        assert_eq!((asphalt.width, asphalt.height), (256, 256));
        assert!(asphalt.non_background_ratio(hex("#E4E4E4")) > 0.05);

        for style in 0..4 {
            let day = draw_facade_day_tile(style);
            assert_eq!((day.width, day.height), (64, 64));
            assert!(day.non_background_ratio(hex("#000000")) > 0.5);

            let night = draw_facade_night_atlas(style);
            assert_eq!((night.width, night.height), (256, 512));
            assert!(night.non_background_ratio(hex("#000000")) > 0.01);
        }

        let strip = draw_generic_shop_strip();
        assert_eq!((strip.width, strip.height), (512, 64));
        assert!(strip.non_background_ratio(hex("#1C2026")) > 0.3);

        let pool = draw_lamp_pool_tex();
        assert_eq!((pool.width, pool.height), (64, 64));
        assert!(pool.rgba[(32 * 64 + 32) * 4 + 3] > 60);

        let com = draw_com_binh_dan_atlas(&mut fonts);
        assert_eq!((com.width, com.height), (1024, 512));
        assert!(com.non_background_ratio([0, 0, 0, 0]) > 0.45);

        let signs = draw_street_signs_atlas(&mut fonts);
        assert_eq!((signs.width, signs.height), (512, 512));
        assert!(signs.non_background_ratio([0, 0, 0, 0]) > 0.5);
    }
}
