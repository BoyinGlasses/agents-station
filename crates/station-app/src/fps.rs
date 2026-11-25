use bevy::prelude::*;
use bevy::window::WindowFocused;
use bevy::winit::{UpdateMode, WinitSettings};
use std::time::Duration;

#[derive(Resource, Debug, Clone)]
pub struct AdaptiveFpsState {
    pub focused: bool,
    pub hidden_to_tray: bool,
    pub capture_mode: bool,
}

impl Default for AdaptiveFpsState {
    fn default() -> Self {
        Self {
            focused: true,
            hidden_to_tray: false,
            capture_mode: false,
        }
    }
}

pub struct AdaptiveFpsPlugin {
    pub capture_mode: bool,
}

impl Plugin for AdaptiveFpsPlugin {
    fn build(&self, app: &mut App) {
        let initial_winit = if self.capture_mode {
            WinitSettings::game()
        } else {
            WinitSettings {
                focused_mode: UpdateMode::reactive(Duration::from_millis(16)), // ~60 fps focused
                unfocused_mode: UpdateMode::reactive_low_power(Duration::from_millis(100)), // 10 fps unfocused
            }
        };
        app.insert_resource(AdaptiveFpsState {
            focused: true,
            hidden_to_tray: false,
            capture_mode: self.capture_mode,
        })
        .insert_resource(initial_winit)
        .add_systems(Update, sync_adaptive_fps_system);
    }
}

fn sync_adaptive_fps_system(
    mut focus_events: EventReader<WindowFocused>,
    mut fps_state: ResMut<AdaptiveFpsState>,
    mut winit: ResMut<WinitSettings>,
    mut ui: Option<ResMut<station_ui::UiState>>,
) {
    if fps_state.capture_mode {
        return;
    }
    let mut changed = fps_state.is_changed();
    for ev in focus_events.read() {
        if fps_state.focused != ev.focused {
            fps_state.focused = ev.focused;
            changed = true;
        }
    }
    if !changed {
        return;
    }
    if fps_state.hidden_to_tray {
        winit.focused_mode = UpdateMode::reactive_low_power(Duration::from_secs(5));
        winit.unfocused_mode = UpdateMode::reactive_low_power(Duration::from_secs(5));
        if let Some(u) = ui.as_mut() {
            u.target_fps = 0;
        }
    } else if fps_state.focused {
        winit.focused_mode = UpdateMode::reactive(Duration::from_millis(16));
        winit.unfocused_mode = UpdateMode::reactive_low_power(Duration::from_millis(100));
        if let Some(u) = ui.as_mut() {
            u.target_fps = 60;
        }
    } else {
        winit.focused_mode = UpdateMode::reactive_low_power(Duration::from_millis(100));
        winit.unfocused_mode = UpdateMode::reactive_low_power(Duration::from_millis(100));
        if let Some(u) = ui.as_mut() {
            u.target_fps = 10;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_fps_transitions_60_to_10_to_0_on_tray() {
        let mut app = App::new();
        app.add_event::<WindowFocused>()
            .insert_resource(station_ui::UiState::default())
            .add_plugins(AdaptiveFpsPlugin {
                capture_mode: false,
            });

        app.update();
        assert_eq!(app.world().resource::<station_ui::UiState>().target_fps, 60);

        app.world_mut().resource_mut::<AdaptiveFpsState>().focused = false;
        app.update();
        assert_eq!(app.world().resource::<station_ui::UiState>().target_fps, 10);

        app.world_mut()
            .resource_mut::<AdaptiveFpsState>()
            .hidden_to_tray = true;
        app.update();
        assert_eq!(app.world().resource::<station_ui::UiState>().target_fps, 0);
    }
}
