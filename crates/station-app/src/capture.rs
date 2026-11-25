use anyhow::{Context, Result, bail};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
}

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CaptureState {
            scene: self.scene.clone(),
            out_path: self.out_path.clone(),
            settle_frames: self.settle_frames.max(3),
            frame_count: 0,
            triggered: false,
        })
        .add_systems(Update, drive_capture);
    }
}

fn drive_capture(
    mut commands: Commands,
    mut state: ResMut<CaptureState>,
    mut exit_events: EventWriter<AppExit>,
) {
    state.frame_count = state.frame_count.saturating_add(1);

    if !state.triggered && state.frame_count >= state.settle_frames {
        state.triggered = true;
        let out_path = state.out_path.clone();
        commands
            .spawn(Screenshot::primary_window())
            .observe(move |trigger: Trigger<ScreenshotCaptured>, mut exit: EventWriter<AppExit>| {
                if let Some(parent) = out_path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    let _ = std::fs::create_dir_all(parent);
                }
                match trigger.event().0.clone().try_into_dynamic() {
                    Ok(dyn_img) => {
                        if let Err(err) = dyn_img.to_rgba8().save(&out_path) {
                            eprintln!("capture save failed ({}): {err}", out_path.display());
                            exit.write(AppExit::error());
                        } else {
                            exit.write(AppExit::Success);
                        }
                    }
                    Err(err) => {
                        eprintln!("capture image conversion failed: {err:?}");
                        exit.write(AppExit::error());
                    }
                }
            });
    } else if state.triggered && state.frame_count > state.settle_frames.saturating_add(300) {
        eprintln!("capture timed out waiting for ScreenshotCaptured");
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
