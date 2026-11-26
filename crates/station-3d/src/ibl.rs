use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

pub const IBL_FACE_SIZE: u32 = 64;
pub const IBL_FACE_COUNT: u32 = 6;
pub const IBL_VRAM_BUDGET_BYTES: usize = 1_572_864; // 1.5 MiB

/// Procedural Studio Day & Night cubemap handles for `EnvironmentMapLight`.
/// All 4 images are `64x64x6` `Rgba16Float` with `RenderAssetUsages::RENDER_WORLD` (~786 KB total).
#[derive(Resource, Debug, Clone)]
pub struct OfficeIbl {
    pub day_diffuse: Handle<Image>,
    pub day_specular: Handle<Image>,
    pub night_diffuse: Handle<Image>,
    pub night_specular: Handle<Image>,
}

impl OfficeIbl {
    pub fn generate(images: &mut Assets<Image>) -> Self {
        Self {
            day_diffuse: build_cubemap(images, sample_day_diffuse),
            day_specular: build_cubemap(images, sample_day_specular),
            night_diffuse: build_cubemap(images, sample_night_diffuse),
            night_specular: build_cubemap(images, sample_night_specular),
        }
    }

    pub fn purge(&self, images: &mut Assets<Image>) {
        images.remove(&self.day_diffuse);
        images.remove(&self.day_specular);
        images.remove(&self.night_diffuse);
        images.remove(&self.night_specular);
    }
}

/// Converts a finite non-negative `f32` to IEEE-754 binary16 bits (`Rgba16Float`).
fn f32_to_f16(val: f32) -> u16 {
    let v = val.clamp(0.0, 65_504.0);
    let bits = v.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x007f_ffff;
    if exp == 0 {
        return 0;
    }
    let half_exp = exp - 127 + 15;
    if half_exp <= 0 {
        if half_exp < -10 {
            return 0;
        }
        let m = (mant | 0x0080_0000) >> (14 - half_exp);
        return ((m + 1) >> 1) as u16;
    }
    if half_exp >= 31 {
        return 0x7bff; // max finite f16
    }
    let rounded = mant + 0x0000_1000;
    if (rounded & 0x0080_0000) != 0 {
        (((half_exp + 1) as u16) << 10).min(0x7bff)
    } else {
        ((half_exp as u16) << 10) | ((rounded >> 13) as u16)
    }
}

/// Standard WebGPU/Vulkan cubemap face directions (+X, -X, +Y, -Y, +Z, -Z).
fn cube_direction(face: usize, u: f32, v: f32) -> Vec3 {
    let d = match face {
        0 => Vec3::new(1.0, -v, -u),  // +X
        1 => Vec3::new(-1.0, -v, u),  // -X
        2 => Vec3::new(u, 1.0, v),    // +Y
        3 => Vec3::new(u, -1.0, -v),  // -Y
        4 => Vec3::new(u, -v, 1.0),   // +Z
        _ => Vec3::new(-u, -v, -1.0), // -Z
    };
    d.normalize()
}

fn softbox_lobe(dir: Vec3, center: Vec3, power: f32, gain: f32) -> f32 {
    dir.dot(center.normalize()).max(0.0).powf(power) * gain
}

/// Smooth day studio irradiance approximating Three.js `RoomEnvironment`.
fn sample_day_diffuse(dir: Vec3) -> Vec3 {
    let up = (dir.y * 0.5 + 0.5).clamp(0.0, 1.0);
    // Warm ceiling (#FFF8EE) to sage-tinted floor bounce (#A9B98A)
    let floor = Vec3::new(0.46, 0.49, 0.41);
    let mid = Vec3::new(0.74, 0.73, 0.70);
    let ceil = Vec3::new(1.05, 1.01, 0.94);
    let base = if up < 0.5 {
        floor.lerp(mid, up * 2.0)
    } else {
        mid.lerp(ceil, (up - 0.5) * 2.0)
    };
    let sun_side = softbox_lobe(dir, Vec3::new(0.6, 0.7, 0.45), 2.0, 0.22);
    base + Vec3::new(1.0, 0.95, 0.86) * sun_side
}

/// Crisp day studio specular map with overhead & side softboxes matching `RoomEnvironment.js`.
fn sample_day_specular(dir: Vec3) -> Vec3 {
    let mut col = sample_day_diffuse(dir) * 0.85;
    // Overhead ceiling softbox (+Y) and 5 wall softboxes from Three's RoomEnvironment.js
    col += Vec3::new(1.0, 0.98, 0.94) * softbox_lobe(dir, Vec3::Y, 14.0, 1.35);
    col += Vec3::new(1.0, 0.97, 0.92) * softbox_lobe(dir, Vec3::new(-0.85, 0.42, 0.32), 16.0, 0.85);
    col += Vec3::new(0.95, 0.97, 1.0) * softbox_lobe(dir, Vec3::new(-0.82, 0.50, -0.35), 16.0, 0.75);
    col += Vec3::new(0.98, 0.96, 0.93) * softbox_lobe(dir, Vec3::new(0.88, 0.45, -0.12), 14.0, 0.55);
    col += Vec3::new(1.0, 0.97, 0.92) * softbox_lobe(dir, Vec3::new(0.0, 0.48, 0.88), 14.0, 0.75);
    col += Vec3::new(0.92, 0.96, 1.0) * softbox_lobe(dir, Vec3::new(0.22, 0.60, -0.78), 16.0, 0.50);
    col
}

/// Smooth box-rect coverage in [0, 1] for crisp-yet-antialiased night panels.
fn rect_coverage(x: f32, y: f32, half_w: f32, half_h: f32, feather: f32) -> f32 {
    let dx = (half_w - x.abs()) / feather;
    let dy = (half_h - y.abs()) / feather;
    dx.clamp(0.0, 1.0) * dy.clamp(0.0, 1.0)
}

/// Night specular cubemap matching `renderer.js:85-93`:
/// 12x12x12 dark blue box `(0.035, 0.05, 0.11)`, warm ceiling `(1.5, 1.0, 0.5)` at `y=5.5` (5x3),
/// cool window `(0.25, 0.35, 0.7)` at `z=-5.8` (4x3, centered at `y=2`).
fn sample_night_specular(dir: Vec3) -> Vec3 {
    let mut col = Vec3::new(0.035, 0.05, 0.11);
    // Warm overhead plane at y = 5.5, size 5x3
    if dir.y > 0.05 {
        let t = 5.5 / dir.y;
        let hit_x = dir.x * t;
        let hit_z = dir.z * t;
        let cov = rect_coverage(hit_x, hit_z, 2.5, 1.5, 0.65);
        col += Vec3::new(1.5, 1.0, 0.5) * cov;
    }
    // Cool window plane at z = -5.8, size 4x3 centered at y = 2.0
    if dir.z < -0.05 {
        let t = -5.8 / dir.z;
        let hit_x = dir.x * t;
        let hit_y = dir.y * t - 2.0;
        let cov = rect_coverage(hit_x, hit_y, 2.0, 1.5, 0.65);
        col += Vec3::new(0.25, 0.35, 0.7) * cov;
    }
    col
}

/// Smooth night diffuse irradiance derived from `renderer.js:85-93`.
fn sample_night_diffuse(dir: Vec3) -> Vec3 {
    let base = Vec3::new(0.045, 0.065, 0.135);
    let warm_ceil = Vec3::new(0.42, 0.28, 0.14) * softbox_lobe(dir, Vec3::Y, 1.6, 1.0);
    let cool_win =
        Vec3::new(0.10, 0.14, 0.28) * softbox_lobe(dir, Vec3::new(0.0, 0.32, -0.95), 2.0, 1.0);
    base + warm_ceil + cool_win
}

fn build_cubemap(images: &mut Assets<Image>, sample: impl Fn(Vec3) -> Vec3) -> Handle<Image> {
    let size = IBL_FACE_SIZE as usize;
    let total_bytes = size * size * (IBL_FACE_COUNT as usize) * 8;
    let mut bytes = Vec::with_capacity(total_bytes);
    let one_f16 = f32_to_f16(1.0).to_le_bytes();

    for face in 0..(IBL_FACE_COUNT as usize) {
        for y in 0..size {
            let v = ((y as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            for x in 0..size {
                let u = ((x as f32 + 0.5) / size as f32) * 2.0 - 1.0;
                let rgb = sample(cube_direction(face, u, v));
                bytes.extend_from_slice(&f32_to_f16(rgb.x).to_le_bytes());
                bytes.extend_from_slice(&f32_to_f16(rgb.y).to_le_bytes());
                bytes.extend_from_slice(&f32_to_f16(rgb.z).to_le_bytes());
                bytes.extend_from_slice(&one_f16);
            }
        }
    }

    let mut img = Image::new(
        Extent3d {
            width: IBL_FACE_SIZE,
            height: IBL_FACE_SIZE,
            depth_or_array_layers: IBL_FACE_COUNT,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::linear();
    img.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    images.add(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureViewDimension};

    fn f16_to_f32(bits: u16) -> f32 {
        let sign = ((bits >> 15) & 1) as u32;
        let exp = ((bits >> 10) & 0x1f) as i32;
        let mant = (bits & 0x3ff) as u32;
        let val = if exp == 0 {
            (mant as f32) * 2.0_f32.powi(-24)
        } else if exp == 31 {
            f32::INFINITY
        } else {
            (1.0 + (mant as f32) / 1024.0) * 2.0_f32.powi(exp - 15)
        };
        if sign == 1 { -val } else { val }
    }

    fn sample_face_center_rgb(img: &Image, face: usize) -> [f32; 3] {
        let size = IBL_FACE_SIZE as usize;
        let cx = size / 2;
        let cy = size / 2;
        let px_idx = (face * size * size + cy * size + cx) * 8;
        let data = img.data.as_ref().expect("image bytes present before GPU upload");
        let r = f16_to_f32(u16::from_le_bytes([data[px_idx], data[px_idx + 1]]));
        let g = f16_to_f32(u16::from_le_bytes([data[px_idx + 2], data[px_idx + 3]]));
        let b = f16_to_f32(u16::from_le_bytes([data[px_idx + 4], data[px_idx + 5]]));
        [r, g, b]
    }

    fn avg_luma(img: &Image) -> f32 {
        let data = img.data.as_ref().expect("image bytes present");
        let px_count = (IBL_FACE_SIZE * IBL_FACE_SIZE * IBL_FACE_COUNT) as usize;
        let mut sum = 0.0_f64;
        for i in 0..px_count {
            let off = i * 8;
            let r = f16_to_f32(u16::from_le_bytes([data[off], data[off + 1]])) as f64;
            let g = f16_to_f32(u16::from_le_bytes([data[off + 2], data[off + 3]])) as f64;
            let b = f16_to_f32(u16::from_le_bytes([data[off + 4], data[off + 5]])) as f64;
            sum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
        }
        (sum / px_count as f64) as f32
    }

    #[test]
    fn office_ibl_generates_four_rgba16f_cubemaps_under_budget() {
        let mut images = Assets::<Image>::default();
        let ibl = OfficeIbl::generate(&mut images);
        assert_eq!(images.len(), 4);

        let handles = [
            &ibl.day_diffuse,
            &ibl.day_specular,
            &ibl.night_diffuse,
            &ibl.night_specular,
        ];
        let mut total_bytes = 0usize;
        for h in handles {
            let img = images.get(h).expect("cubemap handle must exist in Assets<Image>");
            assert_eq!(img.texture_descriptor.size.width, IBL_FACE_SIZE);
            assert_eq!(img.texture_descriptor.size.height, IBL_FACE_SIZE);
            assert_eq!(img.texture_descriptor.size.depth_or_array_layers, IBL_FACE_COUNT);
            assert_eq!(img.texture_descriptor.dimension, TextureDimension::D2);
            assert_eq!(img.texture_descriptor.format, TextureFormat::Rgba16Float);
            assert_eq!(img.asset_usage, RenderAssetUsages::RENDER_WORLD);
            assert_eq!(
                img.texture_view_descriptor.as_ref().and_then(|d| d.dimension),
                Some(TextureViewDimension::Cube)
            );
            let len = img.data.as_ref().map(|d| d.len()).unwrap_or(0);
            assert_eq!(len, (IBL_FACE_SIZE * IBL_FACE_SIZE * IBL_FACE_COUNT * 8) as usize);
            total_bytes += len;
        }
        assert!(
            total_bytes <= IBL_VRAM_BUDGET_BYTES,
            "total IBL bytes {total_bytes} exceeded 1.5 MiB budget"
        );
    }

    #[test]
    fn office_ibl_day_and_night_have_distinct_radiance() {
        let mut images = Assets::<Image>::default();
        let ibl = OfficeIbl::generate(&mut images);

        let day_diff = images.get(&ibl.day_diffuse).unwrap();
        let night_diff = images.get(&ibl.night_diffuse).unwrap();
        let night_spec = images.get(&ibl.night_specular).unwrap();

        assert!(
            avg_luma(day_diff) > avg_luma(night_diff) * 2.5,
            "day diffuse IBL must be significantly brighter than night diffuse"
        );

        // Face 2 is +Y (warm ceiling glow from renderer.js:88: Color(1.5, 1.0, 0.5))
        let [cr, cg, cb] = sample_face_center_rgb(night_spec, 2);
        assert!(cr > cg && cg > cb, "night +Y ceiling must be warm amber, got [{cr}, {cg}, {cb}]");

        // Face 5 is -Z (cool window plane from renderer.js:90: Color(0.25, 0.35, 0.7))
        let [wr, _wg, wb] = sample_face_center_rgb(night_spec, 5);
        assert!(wb > wr, "night -Z face must have cool blue radiance, got R={wr}, B={wb}");
    }

    #[test]
    fn office_ibl_purge_removes_all_four_images() {
        let mut images = Assets::<Image>::default();
        let ibl = OfficeIbl::generate(&mut images);
        assert_eq!(images.len(), 4);
        ibl.purge(&mut images);
        assert_eq!(images.len(), 0);
    }
}
