pub mod boards_and_social;
pub mod brands;
pub mod canvas;
pub mod office_surfaces;
pub mod street_surfaces;
pub mod text;
pub mod wrapped_card;

pub use boards_and_social::{
    DutyBoardInput, DutyRowInput, PhotostripInput, StripPerson, WhiteboardInput, WhiteboardTask,
};
pub use brands::BrandRegistry;
pub use canvas::{Canvas2d, RgbaSurface, hex, hex_rgba};
pub use office_surfaces::{ALL_SCREEN_KINDS, ScreenKind, TvStatsInput};
pub use street_surfaces::SignalColor;
pub use text::{FontEngine, TextAlign};
pub use wrapped_card::{
    PhotoNameTag, WRAPPED_HEIGHT, WRAPPED_WIDTH, WrappedCardInput, draw_wrapped_card,
    encode_rgba_to_png, encode_wrapped_png, paint_photo_overlays,
};

pub fn canvas2d_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn canvas_primitives_and_vietnamese_thai_text_render_cleanly() {
        let mut c = Canvas2d::new(512, 160);
        c.fill(hex("#14161E"));
        c.round_rect(12.0, 12.0, 488.0, 136.0, 14.0, hex("#1E222E"));
        c.linear_gradient_rect(
            24.0,
            24.0,
            464.0,
            10.0,
            (24.0, 24.0),
            (488.0, 24.0),
            &[(0.0, hex("#FF3D9A")), (1.0, hex("#3D7BFF"))],
        );

        let mut fonts = FontEngine::from_assets_dir(&assets_dir());
        let w_vi = fonts.draw_text(
            &mut c,
            "TRỰC NHẬT · BÚN BÒ HUẾ · 5.000đ",
            256.0,
            48.0,
            26.0,
            hex("#FFFFFF"),
            TextAlign::Center,
            Some(460.0),
            true,
            false,
        );
        let w_th = fonts.draw_text(
            &mut c,
            "ภาษาไทย · :4310 OK",
            256.0,
            96.0,
            22.0,
            hex("#3DDC84"),
            TextAlign::Center,
            Some(460.0),
            false,
            true,
        );
        assert!(w_vi > 80.0);
        assert!(w_th > 40.0);

        let surf = c.into_surface();
        assert_eq!(surf.rgba.len(), 512 * 160 * 4);
        assert!(surf.non_background_ratio(hex("#14161E")) > 0.5);
    }
}
