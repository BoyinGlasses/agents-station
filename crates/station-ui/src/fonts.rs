use anyhow::{Context, Result, bail};
use std::path::Path;
use ttf_parser::Face;

#[derive(Debug, Clone)]
pub struct FontBundle {
    pub ui_regular: Vec<Vec<u8>>,
    pub ui_bold: Vec<Vec<u8>>,
    pub mono_regular: Vec<Vec<u8>>,
    pub thai_regular: Vec<u8>,
    pub thai_bold: Vec<u8>,
}

const UI_REGULAR_FILES: [&str; 3] = [
    "fonts/BeVietnamPro-400-latin.ttf",
    "fonts/BeVietnamPro-400-latin-ext.ttf",
    "fonts/BeVietnamPro-400-vietnamese.ttf",
];

const UI_BOLD_FILES: [&str; 3] = [
    "fonts/BeVietnamPro-700-latin.ttf",
    "fonts/BeVietnamPro-700-latin-ext.ttf",
    "fonts/BeVietnamPro-700-vietnamese.ttf",
];

const MONO_REGULAR_FILES: [&str; 3] = [
    "fonts/JetBrainsMono-400-latin.ttf",
    "fonts/JetBrainsMono-400-latin-ext.ttf",
    "fonts/JetBrainsMono-400-vietnamese.ttf",
];

const THAI_REGULAR_FILE: &str = "fonts/PlexThaiLooped-400-thai.ttf";
const THAI_BOLD_FILE: &str = "fonts/PlexThaiLooped-700-thai.ttf";

fn load_chain(assets_dir: &Path, files: &[&str]) -> Result<Vec<Vec<u8>>> {
    files
        .iter()
        .map(|rel| {
            let p = assets_dir.join(rel);
            std::fs::read(&p).with_context(|| format!("missing bundled font {}", p.display()))
        })
        .collect()
}

pub fn load_font_bundle(assets_dir: &Path) -> Result<FontBundle> {
    let thai_path = assets_dir.join(THAI_REGULAR_FILE);
    let thai_bold_path = assets_dir.join(THAI_BOLD_FILE);
    Ok(FontBundle {
        ui_regular: load_chain(assets_dir, &UI_REGULAR_FILES)?,
        ui_bold: load_chain(assets_dir, &UI_BOLD_FILES)?,
        mono_regular: load_chain(assets_dir, &MONO_REGULAR_FILES)?,
        thai_regular: std::fs::read(&thai_path)
            .with_context(|| format!("missing bundled font {}", thai_path.display()))?,
        thai_bold: std::fs::read(&thai_bold_path)
            .with_context(|| format!("missing bundled font {}", thai_bold_path.display()))?,
    })
}

fn check_sample_covered(label: &str, faces: &[Face<'_>], sample: &str) -> Result<()> {
    for ch in sample.chars().filter(|c| !c.is_whitespace()) {
        let found = faces
            .iter()
            .any(|f| f.glyph_index(ch).is_some_and(|gid| gid.0 != 0));
        if !found {
            bail!("{label}: missing glyph for '{ch}' (U+{:04X})", ch as u32);
        }
    }
    Ok(())
}

fn parse_faces(bufs: &[Vec<u8>]) -> Result<Vec<Face<'_>>> {
    bufs.iter()
        .enumerate()
        .map(|(i, b)| Face::parse(b, 0).with_context(|| format!("invalid TTF face #{i}")))
        .collect()
}

pub fn verify_required_glyphs(bundle: &FontBundle) -> Result<()> {
    let ui_reg = parse_faces(&bundle.ui_regular)?;
    let ui_bold = parse_faces(&bundle.ui_bold)?;
    let mono = parse_faces(&bundle.mono_regular)?;
    let thai = Face::parse(&bundle.thai_regular, 0).context("invalid Thai TTF face")?;
    let thai_bold = Face::parse(&bundle.thai_bold, 0).context("invalid Thai bold TTF face")?;

    let ascii = "Hello 0123456789";
    let vi = "Tiếng Việt: Ă Ắ Ặ Đ Ơ Ư Ỹ ệ ổ";
    let th = "ภาษาไทย";

    check_sample_covered("ui_regular ascii", &ui_reg, ascii)?;
    check_sample_covered("ui_regular vietnamese", &ui_reg, vi)?;
    check_sample_covered("ui_bold ascii", &ui_bold, ascii)?;
    check_sample_covered("ui_bold vietnamese", &ui_bold, vi)?;
    check_sample_covered("mono_regular ascii", &mono, ascii)?;
    check_sample_covered("mono_regular vietnamese", &mono, vi)?;
    check_sample_covered("thai_regular thai", &[thai], th)?;
    check_sample_covered("thai_bold thai", &[thai_bold], th)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn bundled_fonts_cover_vietnamese_thai_and_ascii_glyphs() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = load_font_bundle(&assets).expect("load_font_bundle must succeed");
        verify_required_glyphs(&bundle)
            .expect("all Vietnamese, Thai, and ASCII glyphs must exist");
        assert!(assets.join("i18n").exists());
        assert!(assets.join("brands/grab.svg").exists());
        assert!(assets.join("vehicles/cars.json").exists());
        assert!(!bundle.thai_bold.is_empty(), "thai_bold 700 weight must be loaded");
    }
}
