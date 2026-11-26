use crate::icons::{self, IconKind};
use crate::theme::{AMBER, BLUE, GREEN, INK, LINE_2, MUTED, RED, heading_bold};
use bevy::prelude::*;
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};

/// Strict priority ordering for 3D-projected floating stage labels:
/// `Permission > Hovered > WorkingSession > Subagent > ZoneChip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LabelPriority {
    ZoneChip = 0,
    Subagent = 1,
    WorkingSession = 2,
    Hovered = 3,
    Permission = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectedLabelKind {
    WorkerTag,
    SpeechBubble,
    PermissionBadge,
    SubagentTag,
    ZoneChip,
}

#[derive(Debug, Clone)]
pub struct ProjectedLabel {
    pub id: String,
    pub kind: ProjectedLabelKind,
    pub priority: LabelPriority,
    pub screen_pos: Pos2,
    pub depth: f32,
    pub behind_camera: bool,
    pub size: Vec2,
    pub text: String,
    pub subtitle: Option<String>,
    pub accent: Color32,
    pub status: Option<String>,
    pub session_id: Option<String>,
    pub place_id: Option<String>,
    pub icon: Option<IconKind>,
}

impl ProjectedLabel {
    pub fn estimated_rect(&self) -> Rect {
        Rect::from_center_size(self.screen_pos, self.size)
    }
}

/// Interactive click outcome returned by `draw_projected_labels`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelClick {
    Session(String),
    Place(String),
}

/// Projects a 3D world point through `Camera::world_to_viewport`, culling points behind the camera
/// (`world_to_viewport` error or NDC `z < 0.0` / `z > 1.0`) and returning `(screen_pos, depth)`.
pub fn project_world_to_stage(
    world_pos: Vec3,
    camera: &Camera,
    cam_tf: &GlobalTransform,
    stage_rect: Rect,
) -> Option<(Pos2, f32)> {
    if stage_rect.width() < 4.0 || stage_rect.height() < 4.0 {
        return None;
    }
    let ndc = camera.world_to_ndc(cam_tf, world_pos)?;
    if !ndc.is_finite() || ndc.z < 0.0 || ndc.z > 1.0 {
        return None;
    }
    let vp = camera.world_to_viewport(cam_tf, world_pos).ok()?;
    if !vp.is_finite() {
        return None;
    }
    let depth = (world_pos - cam_tf.translation()).dot(cam_tf.forward().as_vec3());
    if !depth.is_finite() || depth < 0.0 {
        return None;
    }
    Some((pos2(vp.x, vp.y), depth))
}

fn overlaps_any(placed: &[Rect], candidate: Rect) -> bool {
    const PAD_X: f32 = 4.0;
    const PAD_Y: f32 = 3.0;
    placed.iter().any(|r| {
        (r.center().x - candidate.center().x).abs() < (r.width() + candidate.width()) * 0.5 + PAD_X
            && (r.center().y - candidate.center().y).abs()
                < (r.height() + candidate.height()) * 0.5 + PAD_Y
    })
}

fn clamp_center_inside(pos: Pos2, size: Vec2, bounds: Rect) -> Pos2 {
    const INSET: f32 = 4.0;
    let hw = size.x * 0.5;
    let hh = size.y * 0.5;
    let min_x = bounds.left() + INSET + hw;
    let max_x = (bounds.right() - INSET - hw).max(min_x);
    let min_y = bounds.top() + INSET + hh;
    let max_y = (bounds.bottom() - INSET - hh).max(min_y);
    pos2(pos.x.clamp(min_x, max_x), pos.y.clamp(min_y, max_y))
}

/// Filters behind-camera and out-of-bounds candidates, sorts by priority descending (then nearer
/// depth), clamps kept pills inside `bounds`, and drops lower-priority overlapping labels matching
/// `my-station/src/office/interact/place-chips.js`.
pub fn declutter_screen_labels(
    candidates: &mut [ProjectedLabel],
    bounds: Rect,
) -> Vec<ProjectedLabel> {
    if bounds.width() < 24.0 || bounds.height() < 24.0 {
        return Vec::new();
    }

    candidates.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.depth.total_cmp(&b.depth))
    });

    let mut placed_rects: Vec<Rect> = Vec::with_capacity(candidates.len());
    let mut kept: Vec<ProjectedLabel> = Vec::with_capacity(candidates.len());
    let max_w = (bounds.width() - 8.0).max(24.0);
    let max_h = (bounds.height() - 8.0).max(18.0);

    for cand in candidates.iter() {
        if cand.behind_camera
            || !cand.depth.is_finite()
            || cand.depth < 0.0
            || !cand.screen_pos.x.is_finite()
            || !cand.screen_pos.y.is_finite()
        {
            continue;
        }
        if !bounds.contains(cand.screen_pos) {
            continue;
        }

        let size = vec2(
            cand.size.x.clamp(24.0, max_w),
            cand.size.y.clamp(18.0, max_h),
        );
        let base_center = clamp_center_inside(cand.screen_pos, size, bounds);

        // Subagents & zone chips never climb; urgent permissions always stay visible.
        let max_steps: usize = match cand.kind {
            ProjectedLabelKind::ZoneChip | ProjectedLabelKind::SubagentTag => 0,
            ProjectedLabelKind::WorkerTag
            | ProjectedLabelKind::SpeechBubble
            | ProjectedLabelKind::PermissionBadge => 3,
        };
        let step_y = size.y + 4.0;
        let mut chosen_center: Option<Pos2> = None;

        for step in 0..=max_steps {
            let shifted = pos2(base_center.x, base_center.y - step as f32 * step_y);
            let test_center = clamp_center_inside(shifted, size, bounds);
            let rect = Rect::from_center_size(test_center, size);
            if !overlaps_any(&placed_rects, rect) {
                chosen_center = Some(test_center);
                break;
            }
        }

        let final_center = match chosen_center {
            Some(pos) => pos,
            None if cand.priority == LabelPriority::Permission => base_center,
            None => continue,
        };

        let mut accepted = cand.clone();
        accepted.screen_pos = final_center;
        accepted.size = size;
        placed_rects.push(Rect::from_center_size(final_center, size));
        kept.push(accepted);
    }

    kept
}

pub fn accent_for_status(status: &str) -> Color32 {
    match status {
        "work" => GREEN,
        "wait" => AMBER,
        "stuck" | "err" => RED,
        "sub" => BLUE,
        _ => MUTED,
    }
}

/// Estimates a pill width/height from its text and kind so decluttering matches rendered sizes.
pub fn estimate_label_size(kind: ProjectedLabelKind, text: &str, subtitle: Option<&str>) -> Vec2 {
    let char_count = text.chars().count() as f32;
    let sub_chars = subtitle.map(|s| s.chars().count() as f32).unwrap_or(0.0);
    match kind {
        ProjectedLabelKind::ZoneChip => {
            let w = (40.0 + char_count * 6.8 + if sub_chars > 0.0 { 14.0 + sub_chars * 5.8 } else { 0.0 })
                .clamp(68.0, 220.0);
            vec2(w, 25.0)
        }
        ProjectedLabelKind::WorkerTag => {
            let w = (36.0 + char_count * 7.2 + if sub_chars > 0.0 { 14.0 + sub_chars * 6.0 } else { 0.0 })
                .clamp(76.0, 210.0);
            vec2(w, 26.0)
        }
        ProjectedLabelKind::SubagentTag => {
            let w = (30.0 + char_count * 6.4).clamp(64.0, 170.0);
            vec2(w, 22.0)
        }
        ProjectedLabelKind::SpeechBubble => {
            let w = (46.0 + char_count * 6.6).clamp(96.0, 210.0);
            vec2(w, 34.0)
        }
        ProjectedLabelKind::PermissionBadge => {
            let w = (44.0 + char_count * 7.0).clamp(110.0, 230.0);
            vec2(w, 30.0)
        }
    }
}

/// Renders decluttered 3D projected labels strictly clipped inside `stage_rect` so pills NEVER
/// spill over `SessionRail`, `TopBar`, or `ActivityPanel`.
pub fn draw_projected_labels(
    ctx: &egui::Context,
    stage_rect: Rect,
    labels: &[ProjectedLabel],
    photo_mode: bool,
    photo_show_names: bool,
    show_zone_chips: bool,
) -> (Vec<LabelClick>, Option<String>) {
    let mut clicks = Vec::new();
    let mut hovered_id = None;

    if labels.is_empty() || stage_rect.width() < 40.0 || stage_rect.height() < 40.0 {
        return (clicks, hovered_id);
    }
    if photo_mode && !photo_show_names {
        return (clicks, hovered_id);
    }

    let mut filtered: Vec<ProjectedLabel> = labels
        .iter()
        .filter(|l| {
            if photo_mode {
                l.kind == ProjectedLabelKind::WorkerTag
            } else if !show_zone_chips && l.kind == ProjectedLabelKind::ZoneChip {
                l.priority >= LabelPriority::Hovered
            } else {
                true
            }
        })
        .cloned()
        .collect();

    // Keep clear of the bottom camera toolbar band (64px) so labels don't collide with buttons.
    let usable_bounds = Rect::from_min_max(
        pos2(stage_rect.left(), stage_rect.top() + 8.0),
        pos2(stage_rect.right(), (stage_rect.bottom() - 68.0).max(stage_rect.top() + 32.0)),
    );
    let visible = declutter_screen_labels(&mut filtered, usable_bounds);
    if visible.is_empty() {
        return (clicks, hovered_id);
    }

    egui::Area::new(egui::Id::new("station_stage_labels3d"))
        .order(egui::Order::Background)
        .fixed_pos(stage_rect.min)
        .show(ctx, |ui| {
            ui.set_clip_rect(stage_rect);
            let painter = ui.painter().with_clip_rect(stage_rect);

            // Paint lower-priority labels first so urgent permissions draw on top.
            for label in visible.iter().rev() {
                let rect = label.estimated_rect();
                let clickable = label.session_id.is_some() || label.place_id.is_some();
                let sense = if clickable { Sense::click() } else { Sense::hover() };
                let resp = ui.interact(rect, egui::Id::new(("lbl3d", &label.id)), sense);
                let is_hot = resp.hovered();
                if is_hot {
                    hovered_id = Some(label.id.clone());
                }

                match label.kind {
                    ProjectedLabelKind::ZoneChip => {
                        paint_zone_chip(&painter, ui, rect, label, is_hot);
                    }
                    ProjectedLabelKind::WorkerTag => {
                        paint_worker_tag(&painter, ui, rect, label, is_hot, false);
                    }
                    ProjectedLabelKind::SubagentTag => {
                        paint_worker_tag(&painter, ui, rect, label, is_hot, true);
                    }
                    ProjectedLabelKind::SpeechBubble => {
                        paint_speech_bubble(&painter, ui, rect, label);
                    }
                    ProjectedLabelKind::PermissionBadge => {
                        paint_permission_badge(&painter, ui, rect, label, is_hot);
                    }
                }

                if resp.clicked() {
                    if let Some(sid) = &label.session_id {
                        clicks.push(LabelClick::Session(sid.clone()));
                    } else if let Some(pid) = &label.place_id {
                        clicks.push(LabelClick::Place(pid.clone()));
                    }
                }
            }
        });

    (clicks, hovered_id)
}

fn paint_zone_chip(
    painter: &egui::Painter,
    ui: &egui::Ui,
    rect: Rect,
    label: &ProjectedLabel,
    hovered: bool,
) {
    let bg = if hovered {
        Color32::from_rgba_unmultiplied(0x14, 0x1B, 0x26, 240)
    } else {
        Color32::from_rgba_unmultiplied(0x0E, 0x13, 0x1B, 178)
    };
    let border = if hovered {
        Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 180)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 28)
    };
    let radius = CornerRadius::same((rect.height() * 0.5).round() as u8);
    painter.rect_filled(rect, radius, bg);
    painter.rect_stroke(rect, radius, Stroke::new(1.0_f32, border), StrokeKind::Inside);

    let ico_center = pos2(rect.left() + 13.0, rect.center().y);
    painter.circle_filled(ico_center, 8.5, Color32::from_rgba_unmultiplied(255, 255, 255, 24));
    let icon_kind = label.icon.unwrap_or(IconKind::Home);
    icons::paint_icon(
        painter,
        Rect::from_center_size(ico_center, vec2(10.0, 10.0)),
        icon_kind,
        crate::theme::ACCENT,
    );

    let text_x = rect.left() + 26.0;
    let title = label.text.to_uppercase();
    let title_galley = ui.fonts(|f| {
        f.layout_no_wrap(
            title,
            FontId::new(9.5, FontFamily::Name(crate::theme::FONT_BOLD.into())),
            if hovered { Color32::WHITE } else { Color32::from_rgba_unmultiplied(0xEC, 0xF0, 0xF7, 224) },
        )
    });
    let ty = rect.center().y - title_galley.size().y * 0.5;
    let title_w = title_galley.size().x;
    painter.galley(pos2(text_x, ty), title_galley, Color32::WHITE);

    if let Some(live) = label.subtitle.as_deref().filter(|s| !s.is_empty()) {
        let sub_galley = ui.fonts(|f| {
            f.layout_no_wrap(
                format!("· {live}"),
                FontId::new(9.5, FontFamily::Proportional),
                GREEN,
            )
        });
        let sy = rect.center().y - sub_galley.size().y * 0.5;
        painter.galley(pos2(text_x + title_w + 5.0, sy), sub_galley, GREEN);
    }
}

fn paint_worker_tag(
    painter: &egui::Painter,
    ui: &egui::Ui,
    rect: Rect,
    label: &ProjectedLabel,
    hovered: bool,
    is_sub: bool,
) {
    let bg = Color32::from_rgba_unmultiplied(0x0E, 0x13, 0x1B, 224);
    let border = if hovered {
        INK
    } else if label.status.as_deref() == Some("wait") {
        AMBER
    } else if label.status.as_deref() == Some("work") {
        Color32::from_rgba_unmultiplied(0x3D, 0xDC, 0x84, 180)
    } else if is_sub {
        Color32::from_rgba_unmultiplied(0x6A, 0xAE, 0xFF, 165)
    } else {
        LINE_2
    };
    let radius = CornerRadius::same((rect.height() * 0.5).round() as u8);
    painter.rect_filled(rect, radius, bg);
    painter.rect_stroke(rect, radius, Stroke::new(1.1_f32, border), StrokeKind::Inside);

    let dot_r = if is_sub { 3.2 } else { 4.0 };
    let dot_center = pos2(rect.left() + 12.0, rect.center().y);
    painter.circle_filled(dot_center, dot_r, label.accent);

    let font_size = if is_sub { 10.5 } else { 11.5 };
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.text.clone(),
            FontId::new(font_size, FontFamily::Name(crate::theme::FONT_BOLD.into())),
            INK,
        )
    });
    let tx = rect.left() + 21.0;
    let ty = rect.center().y - galley.size().y * 0.5;
    let tw = galley.size().x;
    painter.galley(pos2(tx, ty), galley, INK);

    if let Some(sub) = label.subtitle.as_deref().filter(|s| !s.is_empty()) {
        let sub_galley = ui.fonts(|f| {
            f.layout_no_wrap(
                sub.to_owned(),
                FontId::new(9.5, FontFamily::Proportional),
                MUTED,
            )
        });
        let sy = rect.center().y - sub_galley.size().y * 0.5;
        painter.galley(pos2(tx + tw + 6.0, sy), sub_galley, MUTED);
    }
}

fn paint_speech_bubble(
    painter: &egui::Painter,
    ui: &egui::Ui,
    rect: Rect,
    label: &ProjectedLabel,
) {
    let bg = Color32::from_rgba_unmultiplied(0x0E, 0x13, 0x1B, 236);
    let border = label.accent;
    let cr = CornerRadius::same(10);
    painter.rect_filled(rect, cr, bg);
    painter.rect_stroke(rect, cr, Stroke::new(1.1_f32, border), StrokeKind::Inside);

    let ico_rect = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - 10.0), vec2(20.0, 20.0));
    painter.rect_filled(
        ico_rect,
        CornerRadius::same(6),
        Color32::from_rgba_unmultiplied(border.r(), border.g(), border.b(), 42),
    );
    icons::paint_icon(
        painter,
        Rect::from_center_size(ico_rect.center(), vec2(12.0, 12.0)),
        label.icon.unwrap_or(IconKind::Terminal),
        border,
    );

    let max_text_w = (rect.width() - 36.0).max(24.0);
    let galley = ui.fonts(|f| {
        f.layout(
            label.text.clone(),
            FontId::new(11.0, FontFamily::Proportional),
            INK,
            max_text_w,
        )
    });
    let ty = rect.center().y - galley.size().y * 0.5;
    painter.galley(pos2(ico_rect.right() + 6.0, ty), galley, INK);
}

fn paint_permission_badge(
    painter: &egui::Painter,
    ui: &egui::Ui,
    rect: Rect,
    label: &ProjectedLabel,
    hovered: bool,
) {
    let bg = if hovered {
        Color32::from_rgba_unmultiplied(0x2D, 0x22, 0x12, 245)
    } else {
        Color32::from_rgba_unmultiplied(0x1E, 0x18, 0x10, 242)
    };
    let cr = CornerRadius::same(11);
    painter.rect_filled(rect, cr, bg);
    painter.rect_stroke(rect, cr, Stroke::new(1.4_f32, label.accent), StrokeKind::Inside);

    let ico_rect = Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), vec2(13.0, 13.0));
    icons::paint_icon(painter, ico_rect, IconKind::Alert, label.accent);

    let rich = heading_bold(&label.text, 11.0).color(label.accent);
    let galley = egui::WidgetText::from(rich).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (rect.width() - 32.0).max(24.0),
        egui::FontSelection::Default,
    );
    let ty = rect.center().y - galley.size().y * 0.5;
    painter.galley(pos2(rect.left() + 26.0, ty), galley, label.accent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::camera::{CameraProjectionPlugin, ManualTextureViews, RenderTarget, Viewport};
    use bevy::render::render_resource::Extent3d;
    use bevy::window::{WindowCreated, WindowResized, WindowScaleFactorChanged};

    fn make_label(
        id: &str,
        kind: ProjectedLabelKind,
        priority: LabelPriority,
        pos: Pos2,
        depth: f32,
        behind_camera: bool,
    ) -> ProjectedLabel {
        ProjectedLabel {
            id: id.to_owned(),
            kind,
            priority,
            screen_pos: pos,
            depth,
            behind_camera,
            size: vec2(110.0, 26.0),
            text: id.to_owned(),
            subtitle: None,
            accent: GREEN,
            status: Some("work".to_owned()),
            session_id: None,
            place_id: None,
            icon: None,
        }
    }

    #[test]
    fn behind_camera_points_are_dropped_cleanly() {
        let bounds = Rect::from_min_size(pos2(300.0, 56.0), vec2(650.0, 664.0));
        let mut candidates = vec![
            make_label(
                "behind-flag",
                ProjectedLabelKind::WorkerTag,
                LabelPriority::WorkingSession,
                pos2(500.0, 300.0),
                4.0,
                true,
            ),
            make_label(
                "behind-depth",
                ProjectedLabelKind::PermissionBadge,
                LabelPriority::Permission,
                pos2(520.0, 320.0),
                -2.5,
                false,
            ),
            make_label(
                "nan-depth",
                ProjectedLabelKind::ZoneChip,
                LabelPriority::ZoneChip,
                pos2(540.0, 340.0),
                f32::NAN,
                false,
            ),
            make_label(
                "visible-worker",
                ProjectedLabelKind::WorkerTag,
                LabelPriority::WorkingSession,
                pos2(600.0, 350.0),
                8.0,
                false,
            ),
        ];

        let kept = declutter_screen_labels(&mut candidates, bounds);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "visible-worker");
    }

    #[test]
    fn overlapping_lower_priority_labels_are_culled_while_permission_and_worker_remain() {
        assert!(LabelPriority::Permission > LabelPriority::Hovered);
        assert!(LabelPriority::Hovered > LabelPriority::WorkingSession);
        assert!(LabelPriority::WorkingSession > LabelPriority::Subagent);
        assert!(LabelPriority::Subagent > LabelPriority::ZoneChip);

        let bounds = Rect::from_min_size(pos2(300.0, 56.0), vec2(650.0, 664.0));
        let shared = pos2(600.0, 340.0);
        let worker_pos = pos2(600.0, 460.0);
        let mut candidates = vec![
            make_label(
                "zone-meeting",
                ProjectedLabelKind::ZoneChip,
                LabelPriority::ZoneChip,
                shared,
                3.0,
                false,
            ),
            make_label(
                "sub-1",
                ProjectedLabelKind::SubagentTag,
                LabelPriority::Subagent,
                pos2(shared.x + 12.0, shared.y + 4.0),
                4.0,
                false,
            ),
            make_label(
                "perm-urgent",
                ProjectedLabelKind::PermissionBadge,
                LabelPriority::Permission,
                shared,
                12.0,
                false,
            ),
            make_label(
                "worker-claude",
                ProjectedLabelKind::WorkerTag,
                LabelPriority::WorkingSession,
                worker_pos,
                7.0,
                false,
            ),
            make_label(
                "sub-under-worker",
                ProjectedLabelKind::SubagentTag,
                LabelPriority::Subagent,
                pos2(worker_pos.x + 10.0, worker_pos.y + 2.0),
                5.0,
                false,
            ),
        ];

        let kept = declutter_screen_labels(&mut candidates, bounds);
        let ids: Vec<&str> = kept.iter().map(|l| l.id.as_str()).collect();
        assert!(
            ids.contains(&"perm-urgent"),
            "urgent permission badge must survive: {ids:?}"
        );
        assert!(
            ids.contains(&"worker-claude"),
            "worker tag must survive: {ids:?}"
        );
        assert!(
            !ids.contains(&"zone-meeting"),
            "overlapping zone chip must be culled: {ids:?}"
        );
        assert!(
            !ids.contains(&"sub-1"),
            "overlapping subagent under permission must be culled: {ids:?}"
        );
        assert!(
            !ids.contains(&"sub-under-worker"),
            "overlapping subagent under worker must be culled: {ids:?}"
        );
    }

    #[test]
    fn out_of_bounds_stage_points_are_dropped_and_near_edge_pills_are_clamped_inside_bounds() {
        let bounds = Rect::from_min_size(pos2(300.0, 56.0), vec2(650.0, 664.0));
        let mut candidates = vec![
            make_label(
                "in-session-rail",
                ProjectedLabelKind::WorkerTag,
                LabelPriority::Permission,
                pos2(120.0, 300.0),
                5.0,
                false,
            ),
            make_label(
                "in-activity-panel",
                ProjectedLabelKind::WorkerTag,
                LabelPriority::Permission,
                pos2(1050.0, 300.0),
                5.0,
                false,
            ),
            make_label(
                "near-left-edge",
                ProjectedLabelKind::WorkerTag,
                LabelPriority::WorkingSession,
                pos2(305.0, 260.0),
                6.0,
                false,
            ),
        ];

        let kept = declutter_screen_labels(&mut candidates, bounds);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "near-left-edge");
        let rect = kept[0].estimated_rect();
        assert!(
            rect.left() >= bounds.left() && rect.right() <= bounds.right(),
            "pill horizontally spilled out of stage bounds: rect={rect:?}, bounds={bounds:?}"
        );
        assert!(
            rect.top() >= bounds.top() && rect.bottom() <= bounds.bottom(),
            "pill vertically spilled out of stage bounds: rect={rect:?}, bounds={bounds:?}"
        );
    }

    #[test]
    fn project_world_to_stage_culls_behind_camera_and_projects_in_front_points() {
        let stage_rect = Rect::from_min_size(pos2(300.0, 56.0), vec2(650.0, 664.0));
        let mut app = App::new();
        app.insert_resource(Assets::<Image>::default())
            .insert_resource(ManualTextureViews::default())
            .add_event::<WindowResized>()
            .add_event::<WindowScaleFactorChanged>()
            .add_event::<WindowCreated>()
            .add_event::<AssetEvent<Image>>()
            .add_plugins((TransformPlugin, CameraProjectionPlugin));

        let mut img = Image::default();
        img.texture_descriptor.size = Extent3d {
            width: 1280,
            height: 720,
            depth_or_array_layers: 1,
        };
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(img);

        let cam_entity = app
            .world_mut()
            .spawn((
                Camera {
                    target: RenderTarget::Image(handle.into()),
                    viewport: Some(Viewport {
                        physical_position: UVec2::new(300, 56),
                        physical_size: UVec2::new(650, 664),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Projection::Perspective(PerspectiveProjection {
                    fov: 70.0_f32.to_radians(),
                    near: 0.1,
                    far: 200.0,
                    ..Default::default()
                }),
                Transform::from_xyz(0.0, 2.0, 10.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y),
            ))
            .id();
        app.update();

        let cam = app.world().get::<Camera>(cam_entity).unwrap();
        let cam_tf = app.world().get::<GlobalTransform>(cam_entity).unwrap();

        let (pos, depth) = project_world_to_stage(Vec3::new(0.0, 2.0, 0.0), cam, cam_tf, stage_rect)
            .expect("in-front world point must project onto stage");
        assert!((pos.x - stage_rect.center().x).abs() < 2.0, "got pos={pos:?}");
        assert!((pos.y - stage_rect.center().y).abs() < 2.0, "got pos={pos:?}");
        assert!((depth - 10.0).abs() < 0.2, "got depth={depth}");

        assert!(
            project_world_to_stage(Vec3::new(0.0, 2.0, 20.0), cam, cam_tf, stage_rect).is_none(),
            "behind-camera point must return None"
        );
    }
}
