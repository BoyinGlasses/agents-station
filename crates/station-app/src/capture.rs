use anyhow::{Context, Result, bail};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, LoadOp, TextureFormat, TextureUsages};
use bevy_egui::{EguiContext, EguiMultipassSchedule, EguiRenderToImage};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const CAPTURE_WIDTH: u32 = 1280;
const CAPTURE_HEIGHT: u32 = 720;

pub struct CapturePlugin {
    pub scene: String,
    pub out_path: PathBuf,
    pub settle_frames: u32,
}

#[derive(Resource)]
struct CaptureState {
    #[allow(dead_code)]
    scene: String,
    out_path: PathBuf,
    settle_frames: u32,
    frame_count: u32,
    triggered: bool,
    target_image: Option<Handle<Image>>,
}

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CaptureState {
            scene: self.scene.clone(),
            out_path: self.out_path.clone(),
            settle_frames: self.settle_frames.max(4),
            frame_count: 0,
            triggered: false,
            target_image: None,
        })
        .add_systems(PostStartup, setup_capture_target)
        .add_systems(Update, drive_capture);
    }
}

fn setup_capture_target(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<CaptureState>,
    mut cameras: Query<&mut Camera, With<Camera3d>>,
) {
    let size = Extent3d {
        width: CAPTURE_WIDTH,
        height: CAPTURE_HEIGHT,
        depth_or_array_layers: 1,
    };
    let mut image = Image {
        data: Some(vec![0; (CAPTURE_WIDTH * CAPTURE_HEIGHT * 4) as usize]),
        ..default()
    };
    image.texture_descriptor.size = size;
    image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    image.texture_descriptor.usage |=
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;

    let handle = images.add(image);
    for mut cam in &mut cameras {
        cam.target = RenderTarget::Image(handle.clone().into());
    }

    commands.spawn((
        EguiContext::default(),
        EguiRenderToImage {
            handle: handle.clone(),
            load_op: LoadOp::Load,
        },
        EguiMultipassSchedule::new(station_ui::CaptureUiPass),
    ));

    state.target_image = Some(handle);
}

fn drive_capture(
    mut commands: Commands,
    mut state: ResMut<CaptureState>,
    mut exit_events: EventWriter<AppExit>,
) {
    state.frame_count = state.frame_count.saturating_add(1);

    if !state.triggered && state.frame_count >= state.settle_frames {
        let Some(handle) = state.target_image.clone() else {
            return;
        };
        state.triggered = true;
        let out_path = state.out_path.clone();
        commands
            .spawn(Readback::texture(handle))
            .observe(move |trigger: Trigger<ReadbackComplete>, mut exit: EventWriter<AppExit>| {
                if let Some(parent) = out_path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    let _ = std::fs::create_dir_all(parent);
                }
                let raw = &trigger.event().0;
                let row_bytes = (CAPTURE_WIDTH * 4) as usize;
                let aligned_row = row_bytes.div_ceil(256) * 256;
                let mut pixels = Vec::with_capacity(row_bytes * CAPTURE_HEIGHT as usize);
                for y in 0..CAPTURE_HEIGHT as usize {
                    let start = y * aligned_row;
                    let end = start + row_bytes;
                    if end <= raw.len() {
                        pixels.extend_from_slice(&raw[start..end]);
                    }
                }
                match image::RgbaImage::from_raw(CAPTURE_WIDTH, CAPTURE_HEIGHT, pixels) {
                    Some(img) => {
                        if let Err(err) = img.save(&out_path) {
                            eprintln!("capture save failed ({}): {err}", out_path.display());
                            exit.write(AppExit::error());
                        } else {
                            exit.write(AppExit::Success);
                        }
                    }
                    None => {
                        eprintln!(
                            "capture buffer size mismatch: got {} bytes for {}x{}",
                            raw.len(),
                            CAPTURE_WIDTH,
                            CAPTURE_HEIGHT
                        );
                        exit.write(AppExit::error());
                    }
                }
            });
    } else if state.triggered && state.frame_count > state.settle_frames.saturating_add(300) {
        eprintln!("capture timed out waiting for ReadbackComplete");
        exit_events.write(AppExit::error());
    }
}

pub fn validate_non_empty_png(path: &Path) -> Result<(u32, u32)> {
    let img = image::open(path)
        .with_context(|| format!("failed to open PNG {}", path.display()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    if w < 320 || h < 240 {
        bail!("captured PNG too small: {w}x{h}");
    }
    let total = (w as usize) * (h as usize);
    let non_black = img
        .pixels()
        .filter(|p| p[0] > 10 || p[1] > 10 || p[2] > 10)
        .count();
    if non_black * 20 <= total {
        bail!("captured PNG appears mostly black ({non_black}/{total})");
    }
    let buckets: HashSet<(u8, u8, u8)> = img
        .pixels()
        .map(|p| (p[0] / 32, p[1] / 32, p[2] / 32))
        .collect();
    if buckets.len() < 3 {
        bail!("captured PNG appears uniform single color ({} buckets)", buckets.len());
    }
    Ok((w, h))
}
