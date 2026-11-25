use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowFocused, WindowOccluded};
use bevy::winit::{UpdateMode, WinitSettings, WinitWindows};
use std::time::Duration;

#[derive(Resource, Debug, Clone)]
pub struct AdaptiveFpsState {
    pub focused: bool,
    pub minimized: bool,
    pub occluded: bool,
    pub hidden_to_tray: bool,
    pub capture_mode: bool,
}

impl Default for AdaptiveFpsState {
    fn default() -> Self {
        Self {
            focused: true,
            minimized: false,
            occluded: false,
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
        app.init_resource::<Events<WindowFocused>>()
            .init_resource::<Events<WindowOccluded>>()
            .insert_resource(AdaptiveFpsState {
                focused: true,
                minimized: false,
                occluded: false,
                hidden_to_tray: false,
                capture_mode: self.capture_mode,
            })
            .insert_resource(initial_winit)
            .add_systems(Update, sync_adaptive_fps_system);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_adaptive_fps_system(
    mut focus_events: EventReader<WindowFocused>,
    mut occluded_events: EventReader<WindowOccluded>,
    primary_window: Query<(Entity, &Window), With<PrimaryWindow>>,
    winit_windows: Option<NonSend<WinitWindows>>,
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
    for ev in occluded_events.read() {
        if fps_state.occluded != ev.occluded {
            fps_state.occluded = ev.occluded;
            changed = true;
        }
    }
    if let Ok((ent, win)) = primary_window.single() {
        let winit_min = winit_windows
            .as_deref()
            .and_then(|ww| ww.get_window(ent))
            .and_then(|w| w.is_minimized())
            .unwrap_or(false);
        let is_min = winit_min
            || !win.visible
            || win.resolution.physical_width() == 0
            || win.resolution.physical_height() == 0;
        if fps_state.minimized != is_min {
            fps_state.minimized = is_min;
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
    } else if fps_state.minimized || fps_state.occluded {
        winit.focused_mode = UpdateMode::reactive_low_power(Duration::from_millis(500));
        winit.unfocused_mode = UpdateMode::reactive_low_power(Duration::from_millis(500));
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

    #[test]
    fn adaptive_fps_drops_to_zero_and_500ms_idle_when_minimized_or_occluded() {
        use bevy::window::{PrimaryWindow, WindowOccluded, WindowResolution};

        let mut app = App::new();
        app.add_event::<WindowFocused>()
            .add_event::<WindowOccluded>()
            .insert_resource(station_ui::UiState::default())
            .add_plugins(AdaptiveFpsPlugin {
                capture_mode: false,
            });

        let win_ent = app
            .world_mut()
            .spawn((
                PrimaryWindow,
                Window {
                    resolution: WindowResolution::new(1280.0, 720.0),
                    ..default()
                },
            ))
            .id();

        app.update();
        assert_eq!(app.world().resource::<station_ui::UiState>().target_fps, 60);

        // Minimized window (0x0 physical resolution) drops to 0 FPS with 500ms idle wait.
        app.world_mut()
            .entity_mut(win_ent)
            .get_mut::<Window>()
            .unwrap()
            .resolution = WindowResolution::new(0.0, 0.0);
        app.update();
        assert_eq!(
            app.world().resource::<station_ui::UiState>().target_fps,
            0,
            "minimized window must drop target_fps to 0"
        );
        assert_eq!(
            app.world().resource::<WinitSettings>().focused_mode,
            UpdateMode::reactive_low_power(Duration::from_millis(500))
        );
        assert_eq!(
            app.world().resource::<WinitSettings>().unfocused_mode,
            UpdateMode::reactive_low_power(Duration::from_millis(500))
        );

        // Restoring window returns to 60 FPS.
        app.world_mut()
            .entity_mut(win_ent)
            .get_mut::<Window>()
            .unwrap()
            .resolution = WindowResolution::new(1280.0, 720.0);
        app.update();
        assert_eq!(app.world().resource::<station_ui::UiState>().target_fps, 60);

        // WindowOccluded(true) also drops to 0 FPS with 500ms idle wait.
        app.world_mut().send_event(WindowOccluded {
            window: win_ent,
            occluded: true,
        });
        app.update();
        assert_eq!(
            app.world().resource::<station_ui::UiState>().target_fps,
            0,
            "occluded window must drop target_fps to 0"
        );
        assert_eq!(
            app.world().resource::<WinitSettings>().focused_mode,
            UpdateMode::reactive_low_power(Duration::from_millis(500))
        );

        // WindowOccluded(false) restores 60 FPS.
        app.world_mut().send_event(WindowOccluded {
            window: win_ent,
            occluded: false,
        });
        app.update();
        assert_eq!(app.world().resource::<station_ui::UiState>().target_fps, 60);
    }
}
