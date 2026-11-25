use crate::canvas::RgbaSurface;
use anyhow::{Context, Result, bail};
use resvg::usvg::{Options, Tree};
use std::collections::HashMap;
use std::path::PathBuf;
use tiny_skia::{Pixmap, Transform};

pub const BRAND_SVG_FILES: &[&str] = &[
    "7-eleven.svg",
    "circle-k.svg",
    "grab.svg",
    "gs25.svg",
    "ministop.svg",
    "xanh-sm.svg",
];

pub const BRAND_PNG_FILES: &[&str] = &["bach-hoa-xanh.png", "winmart.png"];

pub struct BrandRegistry {
    assets_dir: PathBuf,
    cache: HashMap<(String, u32, u32), RgbaSurface>,
}

impl BrandRegistry {
    pub fn new(assets_dir: PathBuf) -> Self {
        Self {
            assets_dir,
            cache: HashMap::new(),
        }
    }

    /// Drops all cached brand RGBA surfaces to free CPU RAM after GPU upload.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }

    /// Rasterizes a brand SVG (via `resvg`) or decodes a brand PNG into an RGBA surface
    /// fitting within `(max_w, max_h)` while preserving aspect ratio.
    pub fn load_brand_rgba(
        &mut self,
        file: &str,
        max_w: u32,
        max_h: u32,
    ) -> Result<RgbaSurface> {
        let key = (file.to_owned(), max_w, max_h);
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit.clone());
        }
        let path = self.assets_dir.join("brands").join(file);
        let surf = if file.ends_with(".svg") {
            rasterize_svg(&path, max_w, max_h)?
        } else {
            decode_image_rgba(&path)?
        };
        if self.cache.len() < 32 {
            self.cache.insert(key, surf.clone());
        }
        Ok(surf)
    }

    pub fn load_thumb_rgba(&self, layout: &str) -> Result<RgbaSurface> {
        let fname = match layout {
            "cozy" => "layout-cozy.jpg",
            "pro" => "layout-pro.jpg",
            "hq" => "layout-hq.jpg",
            other => bail!("unknown layout thumb: {other}"),
        };
        decode_image_rgba(&self.assets_dir.join("thumbs").join(fname))
    }

    pub fn load_app_icon(&self) -> Result<RgbaSurface> {
        decode_image_rgba(&self.assets_dir.join("icon-512.png"))
    }

    pub fn load_vehicle_catalog(&self) -> Result<serde_json::Value> {
        let path = self.assets_dir.join("vehicles").join("cars.json");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }
}

fn rasterize_svg(path: &std::path::Path, max_w: u32, max_h: u32) -> Result<RgbaSurface> {
    let data = std::fs::read(path).with_context(|| format!("read svg {}", path.display()))?;
    let opt = Options::default();
    let tree = Tree::from_data(&data, &opt)
        .with_context(|| format!("parse svg {}", path.display()))?;
    let size = tree.size();
    let sw = size.width().max(1.0);
    let sh = size.height().max(1.0);
    let scale = (max_w.max(1) as f32 / sw).min(max_h.max(1) as f32 / sh);
    let pw = ((sw * scale).round() as u32).max(1);
    let ph = ((sh * scale).round() as u32).max(1);

    let mut pixmap = Pixmap::new(pw, ph).context("alloc svg pixmap")?;
    let ts = Transform::from_scale(pw as f32 / sw, ph as f32 / sh);
    resvg::render(&tree, ts, &mut pixmap.as_mut());

    // Un-premultiply straight RGBA
    let raw = pixmap.take();
    let mut rgba = Vec::with_capacity(raw.len());
    for px in raw.chunks_exact(4) {
        let a = px[3];
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            rgba.extend_from_slice(px);
        } else {
            let af = a as f32 / 255.0;
            rgba.push(((px[0] as f32 / af).round()).min(255.0) as u8);
            rgba.push(((px[1] as f32 / af).round()).min(255.0) as u8);
            rgba.push(((px[2] as f32 / af).round()).min(255.0) as u8);
            rgba.push(a);
        }
    }
    Ok(RgbaSurface {
        width: pw,
        height: ph,
        rgba,
    })
}

fn decode_image_rgba(path: &std::path::Path) -> Result<RgbaSurface> {
    let img = image::open(path)
        .with_context(|| format!("decode image {}", path.display()))?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Ok(RgbaSurface {
        width,
        height,
        rgba: img.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn loads_all_brand_svgs_pngs_thumbs_icon_and_vehicle_catalog_and_clears() {
        let mut reg = BrandRegistry::new(assets_dir());

        for svg in BRAND_SVG_FILES {
            let surf = reg.load_brand_rgba(svg, 256, 128).unwrap();
            assert!(surf.width > 0 && surf.width <= 256, "{svg} width={}", surf.width);
            assert!(surf.height > 0 && surf.height <= 128, "{svg} height={}", surf.height);
            assert!(surf.non_background_ratio([0, 0, 0, 0]) > 0.05, "{svg} empty");
        }
        for png in BRAND_PNG_FILES {
            let surf = reg.load_brand_rgba(png, 256, 128).unwrap();
            assert!(surf.width > 0 && surf.height > 0, "{png} empty");
        }
        assert!(reg.cached_count() >= 8);
        reg.clear();
        assert_eq!(reg.cached_count(), 0);

        for lay in ["cozy", "pro", "hq"] {
            let thumb = reg.load_thumb_rgba(lay).unwrap();
            assert!(thumb.width >= 64 && thumb.height >= 64, "thumb {lay}");
        }
        let icon = reg.load_app_icon().unwrap();
        assert_eq!(icon.width, 512);
        assert_eq!(icon.height, 512);

        let cars = reg.load_vehicle_catalog().unwrap();
        assert!(cars.is_object() || cars.is_array());
    }
}
