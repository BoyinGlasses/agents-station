#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellLayout {
    pub show_topbar: bool,
    pub show_session_rail: bool,
    pub show_stage_chips: bool,
    pub show_activity_panel: bool,
    pub show_activity_bottom_sheet: bool,
    pub topbar_height: f32,
    pub left_width: f32,
    pub right_width: f32,
    pub compact_topbar_labels: bool,
}

pub fn compute_shell_layout(
    viewport_width: f32,
    fullscreen: bool,
    photo_mode: bool,
) -> ShellLayout {
    if fullscreen || photo_mode {
        return ShellLayout {
            show_topbar: false,
            show_session_rail: false,
            show_stage_chips: false,
            show_activity_panel: false,
            show_activity_bottom_sheet: false,
            topbar_height: 0.0,
            left_width: 0.0,
            right_width: 0.0,
            compact_topbar_labels: true,
        };
    }

    let compact_topbar_labels = viewport_width <= 1560.0;

    if viewport_width < 720.0 {
        ShellLayout {
            show_topbar: true,
            show_session_rail: false,
            show_stage_chips: true,
            show_activity_panel: false,
            show_activity_bottom_sheet: true,
            topbar_height: 52.0,
            left_width: 0.0,
            right_width: 0.0,
            compact_topbar_labels: true,
        }
    } else if viewport_width < 1100.0 {
        ShellLayout {
            show_topbar: true,
            show_session_rail: false,
            show_stage_chips: true,
            show_activity_panel: true,
            show_activity_bottom_sheet: false,
            topbar_height: 56.0,
            left_width: 0.0,
            right_width: 320.0,
            compact_topbar_labels,
        }
    } else if viewport_width <= 1440.0 {
        ShellLayout {
            show_topbar: true,
            show_session_rail: true,
            show_stage_chips: false,
            show_activity_panel: true,
            show_activity_bottom_sheet: false,
            topbar_height: 56.0,
            left_width: 300.0,
            right_width: 330.0,
            compact_topbar_labels,
        }
    } else {
        ShellLayout {
            show_topbar: true,
            show_session_rail: true,
            show_stage_chips: false,
            show_activity_panel: true,
            show_activity_bottom_sheet: false,
            topbar_height: 56.0,
            left_width: 330.0,
            right_width: 350.0,
            compact_topbar_labels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_desktop_shows_three_column_shell_with_exact_widths() {
        let l = compute_shell_layout(1600.0, false, false);
        assert!(l.show_topbar);
        assert!(l.show_session_rail);
        assert!(!l.show_stage_chips);
        assert!(l.show_activity_panel);
        assert!(!l.show_activity_bottom_sheet);
        assert_eq!(l.topbar_height, 56.0);
        assert_eq!(l.left_width, 330.0);
        assert_eq!(l.right_width, 350.0);
        assert!(!l.compact_topbar_labels);
    }

    #[test]
    fn medium_desktop_1280_tightens_sidebars_and_hides_topbar_button_labels() {
        let l = compute_shell_layout(1280.0, false, false);
        assert!(l.show_topbar);
        assert!(l.show_session_rail);
        assert!(!l.show_stage_chips);
        assert!(l.show_activity_panel);
        assert_eq!(l.topbar_height, 56.0);
        assert_eq!(l.left_width, 300.0);
        assert_eq!(l.right_width, 330.0);
        assert!(l.compact_topbar_labels);
    }

    #[test]
    fn below_1100px_collapses_session_rail_to_stage_chips() {
        let l = compute_shell_layout(1050.0, false, false);
        assert!(l.show_topbar);
        assert!(!l.show_session_rail);
        assert!(l.show_stage_chips);
        assert!(l.show_activity_panel);
        assert!(!l.show_activity_bottom_sheet);
        assert_eq!(l.left_width, 0.0);
        assert_eq!(l.right_width, 320.0);
    }

    #[test]
    fn below_720px_collapses_activity_panel_to_bottom_sheet() {
        let l = compute_shell_layout(640.0, false, false);
        assert!(l.show_topbar);
        assert_eq!(l.topbar_height, 52.0);
        assert!(!l.show_session_rail);
        assert!(l.show_stage_chips);
        assert!(!l.show_activity_panel);
        assert!(l.show_activity_bottom_sheet);
        assert_eq!(l.left_width, 0.0);
        assert_eq!(l.right_width, 0.0);
    }

    #[test]
    fn fullscreen_and_photo_mode_hide_all_chrome() {
        for (fs, photo) in [(true, false), (false, true), (true, true)] {
            let l = compute_shell_layout(1600.0, fs, photo);
            assert!(!l.show_topbar);
            assert!(!l.show_session_rail);
            assert!(!l.show_stage_chips);
            assert!(!l.show_activity_panel);
            assert!(!l.show_activity_bottom_sheet);
            assert_eq!(l.topbar_height, 0.0);
            assert_eq!(l.left_width, 0.0);
            assert_eq!(l.right_width, 0.0);
        }
    }
}
