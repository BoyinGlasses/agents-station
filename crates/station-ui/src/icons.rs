use bevy_egui::egui::{self, Color32, CornerRadius, Pos2, Rect, Shape, Stroke, StrokeKind, vec2};
use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconKind {
    Logo,
    Home,
    Monitor,
    Users,
    Server,
    Walk,
    Camera,
    List,
    Maximize,
    Chat,
    Rewind,
    Gear,
    Phone,
    Moon,
    Sun,
    Search,
    Terminal,
    Pencil,
    File,
    Globe,
    Trophy,
    Timeline,
    Apps,
    Close,
    Check,
    Alert,
}

impl IconKind {
    pub const ALL: [IconKind; 26] = [
        IconKind::Logo,
        IconKind::Home,
        IconKind::Monitor,
        IconKind::Users,
        IconKind::Server,
        IconKind::Walk,
        IconKind::Camera,
        IconKind::List,
        IconKind::Maximize,
        IconKind::Chat,
        IconKind::Rewind,
        IconKind::Gear,
        IconKind::Phone,
        IconKind::Moon,
        IconKind::Sun,
        IconKind::Search,
        IconKind::Terminal,
        IconKind::Pencil,
        IconKind::File,
        IconKind::Globe,
        IconKind::Trophy,
        IconKind::Timeline,
        IconKind::Apps,
        IconKind::Close,
        IconKind::Check,
        IconKind::Alert,
    ];
}

struct Canvas<'a> {
    painter: &'a egui::Painter,
    rect: Rect,
    color: Color32,
    stroke: Stroke,
    scale: f32,
}

impl<'a> Canvas<'a> {
    fn new(painter: &'a egui::Painter, rect: Rect, color: Color32) -> Self {
        let scale = rect.width().min(rect.height()) / 24.0;
        let stroke_w = (2.0 * scale).max(1.25);
        Self {
            painter,
            rect,
            color,
            stroke: Stroke::new(stroke_w, color),
            scale,
        }
    }

    fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.rect.min
            + vec2(
                x / 24.0 * self.rect.width(),
                y / 24.0 * self.rect.height(),
            )
    }

    fn subrect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_max(self.pt(x, y), self.pt(x + w, y + h))
    }

    fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.painter
            .line_segment([self.pt(x1, y1), self.pt(x2, y2)], self.stroke);
    }

    fn polyline(&self, pts: &[(f32, f32)]) {
        let mapped = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.painter.add(Shape::line(mapped, self.stroke));
    }

    fn polygon_stroke(&self, pts: &[(f32, f32)]) {
        let mapped = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.painter
            .add(Shape::closed_line(mapped, self.stroke));
    }

    fn rect_stroke(&self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let cr = CornerRadius::same((r * self.scale).round() as u8);
        self.painter
            .rect_stroke(self.subrect(x, y, w, h), cr, self.stroke, StrokeKind::Middle);
    }

    fn rect_filled(&self, x: f32, y: f32, w: f32, h: f32, cr: CornerRadius) {
        self.painter
            .rect_filled(self.subrect(x, y, w, h), cr, self.color);
    }

    fn circle_stroke(&self, cx: f32, cy: f32, r: f32) {
        self.painter
            .circle_stroke(self.pt(cx, cy), r * self.scale, self.stroke);
    }

    fn circle_filled(&self, cx: f32, cy: f32, r: f32) {
        self.painter
            .circle_filled(self.pt(cx, cy), (r * self.scale).max(1.0), self.color);
    }

    fn arc(&self, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, steps: usize) {
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let a = a0 + (a1 - a0) * t;
                self.pt(cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect();
        self.painter.add(Shape::line(pts, self.stroke));
    }
}

/// Paints a crisp vector icon scaled from the 24x24 `icons.js` SVG grid into `rect`.
pub fn paint_icon(painter: &egui::Painter, rect: Rect, kind: IconKind, color: Color32) {
    let c = Canvas::new(painter, rect, color);
    match kind {
        IconKind::Logo => {
            // OFFICE_MARK building silhouette with transparent windows and doorway.
            let top_r = (3.0 * c.scale).round() as u8;
            let roof_cr = CornerRadius {
                nw: top_r,
                ne: top_r,
                sw: 0,
                se: 0,
            };
            let sq = CornerRadius::ZERO;
            c.rect_filled(4.0, 2.0, 16.0, 4.0, roof_cr);
            // Upper window band (windows at 7.5..10.5 and 13.5..16.5)
            c.rect_filled(4.0, 6.0, 3.5, 3.0, sq);
            c.rect_filled(10.5, 6.0, 3.0, 3.0, sq);
            c.rect_filled(16.5, 6.0, 3.5, 3.0, sq);
            c.rect_filled(4.0, 9.0, 16.0, 2.0, sq);
            // Lower window band
            c.rect_filled(4.0, 11.0, 3.5, 3.0, sq);
            c.rect_filled(10.5, 11.0, 3.0, 3.0, sq);
            c.rect_filled(16.5, 11.0, 3.5, 3.0, sq);
            c.rect_filled(4.0, 14.0, 16.0, 3.0, sq);
            // Doorway band (door at 10.2..13.8)
            c.rect_filled(4.0, 17.0, 6.2, 5.0, sq);
            c.rect_filled(13.8, 17.0, 6.2, 5.0, sq);
        }
        IconKind::Home => {
            c.polygon_stroke(&[
                (3.0, 9.0),
                (12.0, 2.0),
                (21.0, 9.0),
                (21.0, 20.0),
                (19.5, 22.0),
                (4.5, 22.0),
                (3.0, 20.0),
            ]);
            c.polyline(&[(9.0, 22.0), (9.0, 12.0), (15.0, 12.0), (15.0, 22.0)]);
        }
        IconKind::Monitor => {
            c.rect_stroke(2.0, 3.0, 20.0, 14.0, 2.0);
            c.line(8.0, 21.0, 16.0, 21.0);
            c.line(12.0, 17.0, 12.0, 21.0);
        }
        IconKind::Users => {
            c.circle_stroke(9.0, 7.0, 4.0);
            c.polyline(&[
                (2.0, 21.0),
                (2.0, 19.0),
                (3.5, 16.2),
                (6.0, 15.0),
                (12.0, 15.0),
                (14.5, 16.2),
                (16.0, 19.0),
                (16.0, 21.0),
            ]);
            c.arc(16.0, 7.0, 4.0, 4.0, -1.25, 1.25, 8);
            c.polyline(&[(19.0, 15.2), (21.0, 16.8), (22.0, 19.0), (22.0, 21.0)]);
        }
        IconKind::Server => {
            c.rect_stroke(2.0, 2.0, 20.0, 8.0, 2.0);
            c.rect_stroke(2.0, 14.0, 20.0, 8.0, 2.0);
            c.circle_filled(6.0, 6.0, 1.1);
            c.circle_filled(6.0, 18.0, 1.1);
        }
        IconKind::Walk => {
            c.circle_stroke(13.0, 4.0, 2.0);
            c.polyline(&[(9.0, 20.0), (12.0, 14.0), (15.0, 17.0), (15.0, 21.0)]);
            c.polyline(&[(6.0, 12.0), (9.0, 8.0), (13.0, 9.0), (16.0, 12.0), (19.0, 13.0)]);
        }
        IconKind::Camera => {
            c.rect_stroke(3.0, 7.0, 18.0, 13.0, 2.5);
            c.polyline(&[(8.5, 7.0), (10.0, 4.5), (14.0, 4.5), (15.5, 7.0)]);
            c.circle_stroke(12.0, 13.5, 3.5);
        }
        IconKind::List => {
            c.line(4.0, 6.0, 20.0, 6.0);
            c.line(7.0, 12.0, 17.0, 12.0);
            c.line(10.0, 18.0, 14.0, 18.0);
        }
        IconKind::Maximize => {
            c.polyline(&[(8.0, 3.0), (3.0, 3.0), (3.0, 8.0)]);
            c.polyline(&[(16.0, 3.0), (21.0, 3.0), (21.0, 8.0)]);
            c.polyline(&[(3.0, 16.0), (3.0, 21.0), (8.0, 21.0)]);
            c.polyline(&[(21.0, 16.0), (21.0, 21.0), (16.0, 21.0)]);
        }
        IconKind::Chat => {
            let mut pts = Vec::with_capacity(20);
            pts.push(c.pt(3.0, 21.0));
            let steps = 16;
            let a0 = 2.65_f32;
            let a1 = a0 - TAU + 0.65;
            for i in 0..=steps {
                let a = a0 + (a1 - a0) * (i as f32 / steps as f32);
                pts.push(c.pt(12.0 + 8.6 * a.cos(), 11.5 + 8.4 * a.sin()));
            }
            c.painter.add(Shape::closed_line(pts, c.stroke));
        }
        IconKind::Rewind => {
            c.polygon_stroke(&[(11.0, 19.0), (2.0, 12.0), (11.0, 5.0)]);
            c.polygon_stroke(&[(22.0, 19.0), (13.0, 12.0), (22.0, 5.0)]);
        }
        IconKind::Gear => {
            c.circle_stroke(12.0, 12.0, 3.0);
            c.circle_stroke(12.0, 12.0, 7.2);
            for i in 0..8 {
                let a = i as f32 * (TAU / 8.0);
                c.line(
                    12.0 + 7.2 * a.cos(),
                    12.0 + 7.2 * a.sin(),
                    12.0 + 10.2 * a.cos(),
                    12.0 + 10.2 * a.sin(),
                );
            }
        }
        IconKind::Phone => {
            c.rect_stroke(5.0, 2.0, 14.0, 20.0, 2.0);
            c.circle_filled(12.0, 18.0, 1.1);
        }
        IconKind::Moon => {
            // Outer crescent arc + inner bite matching `M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z`.
            let mut pts = Vec::with_capacity(28);
            let outer_steps = 16;
            for i in 0..=outer_steps {
                let t = i as f32 / outer_steps as f32;
                let a = -std::f32::consts::FRAC_PI_2 - t * 1.5 * std::f32::consts::PI;
                pts.push(c.pt(12.0 + 9.0 * a.cos(), 12.0 + 9.0 * a.sin()));
            }
            let inner_steps = 10;
            for i in 1..inner_steps {
                let t = i as f32 / inner_steps as f32;
                let a = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::FRAC_PI_2;
                pts.push(c.pt(18.0 + 6.0 * a.cos(), 6.0 + 6.0 * a.sin()));
            }
            c.painter.add(Shape::closed_line(pts, c.stroke));
        }
        IconKind::Sun => {
            c.circle_stroke(12.0, 12.0, 4.0);
            for &(x1, y1, x2, y2) in &[
                (12.0, 2.0, 12.0, 4.0),
                (12.0, 20.0, 12.0, 22.0),
                (4.9, 4.9, 6.3, 6.3),
                (17.7, 17.7, 19.1, 19.1),
                (2.0, 12.0, 4.0, 12.0),
                (20.0, 12.0, 22.0, 12.0),
                (4.9, 19.1, 6.3, 17.7),
                (17.7, 6.3, 19.1, 4.9),
            ] {
                c.line(x1, y1, x2, y2);
            }
        }
        IconKind::Search => {
            c.circle_stroke(11.0, 11.0, 7.0);
            c.line(21.0, 21.0, 16.65, 16.65);
        }
        IconKind::Terminal => {
            c.polyline(&[(4.0, 17.0), (10.0, 11.0), (4.0, 5.0)]);
            c.line(12.0, 19.0, 20.0, 19.0);
        }
        IconKind::Pencil => {
            c.line(12.0, 20.0, 21.0, 20.0);
            c.polygon_stroke(&[
                (16.5, 3.5),
                (19.5, 3.5),
                (20.5, 6.5),
                (7.0, 19.0),
                (3.0, 20.0),
                (4.0, 16.0),
            ]);
        }
        IconKind::File => {
            c.polygon_stroke(&[
                (14.0, 2.0),
                (6.0, 2.0),
                (4.0, 4.0),
                (4.0, 20.0),
                (6.0, 22.0),
                (18.0, 22.0),
                (20.0, 20.0),
                (20.0, 8.0),
            ]);
            c.polyline(&[(14.0, 2.0), (14.0, 8.0), (20.0, 8.0)]);
        }
        IconKind::Globe => {
            c.circle_stroke(12.0, 12.0, 9.0);
            c.line(3.0, 12.0, 21.0, 12.0);
            c.arc(12.0, 12.0, 4.2, 9.0, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, 10);
            c.arc(12.0, 12.0, 4.2, 9.0, std::f32::consts::FRAC_PI_2, 1.5 * std::f32::consts::PI, 10);
        }
        IconKind::Trophy => {
            c.polyline(&[(6.0, 2.0), (18.0, 2.0), (18.0, 9.0)]);
            c.arc(12.0, 9.0, 6.0, 6.0, 0.0, std::f32::consts::PI, 10);
            c.line(6.0, 9.0, 6.0, 2.0);
            c.polyline(&[(6.0, 4.0), (3.0, 4.0), (2.5, 6.5), (4.5, 9.0), (6.0, 9.0)]);
            c.polyline(&[(18.0, 4.0), (21.0, 4.0), (21.5, 6.5), (19.5, 9.0), (18.0, 9.0)]);
            c.line(12.0, 15.0, 12.0, 22.0);
            c.line(4.0, 22.0, 20.0, 22.0);
        }
        IconKind::Timeline => {
            c.rect_stroke(3.0, 4.0, 9.0, 4.0, 1.5);
            c.rect_stroke(8.0, 10.0, 13.0, 4.0, 1.5);
            c.rect_stroke(5.0, 16.0, 7.0, 4.0, 1.5);
        }
        IconKind::Apps => {
            c.rect_stroke(3.0, 3.0, 7.5, 7.5, 1.5);
            c.rect_stroke(13.5, 3.0, 7.5, 7.5, 1.5);
            c.rect_stroke(3.0, 13.5, 7.5, 7.5, 1.5);
            c.rect_stroke(13.5, 13.5, 7.5, 7.5, 1.5);
        }
        IconKind::Close => {
            c.line(18.0, 6.0, 6.0, 18.0);
            c.line(6.0, 6.0, 18.0, 18.0);
        }
        IconKind::Check => {
            c.polyline(&[(20.0, 6.0), (9.0, 17.0), (4.0, 12.0)]);
        }
        IconKind::Alert => {
            c.polygon_stroke(&[(12.0, 3.5), (2.2, 20.5), (21.8, 20.5)]);
            c.line(12.0, 9.0, 12.0, 13.5);
            c.circle_filled(12.0, 17.2, 1.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_icon_emits_vector_shapes_for_every_icon_kind_variant() {
        assert_eq!(IconKind::ALL.len(), 26);
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(16.0, 16.0));
        for kind in IconKind::ALL {
            let ctx = egui::Context::default();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("icon_test"),
                ));
                paint_icon(&painter, rect, kind, crate::theme::INK);
            });
            assert!(
                !output.shapes.is_empty(),
                "IconKind::{kind:?} emitted zero vector shapes"
            );
        }
    }
}
