use bevy::prelude::*;
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Resource, Default)]
pub enum OfficeLayout {
    Cozy,
    #[default]
    Pro,
    Hq,
}

impl OfficeLayout {
    pub fn is_pro(self) -> bool {
        matches!(self, Self::Pro | Self::Hq)
    }

    pub fn is_hq(self) -> bool {
        matches!(self, Self::Hq)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cozy => "cozy",
            Self::Pro => "pro",
            Self::Hq => "hq",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FloorRect {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DeptRoom {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    pub cx: f32,
    pub cz: f32,
    pub pods: [[f32; 2]; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct DeskCoord {
    pub x: f32,
    pub z: f32,
    pub ry: f32,
}

#[derive(Debug, Clone)]
pub struct LayoutSpec {
    pub layout: OfficeLayout,
    pub bounds: FloorRect,
    pub reach: FloorRect,
    pub center: Vec2,
    pub home: Vec3,
    pub cam_offset: Vec3,
    pub viewport_height: f32,
    pub shelf: [f32; 3],
    pub globe: [f32; 2],
    pub board: [f32; 4],
    pub door_z: f32,
    pub clock: [f32; 4],
    pub racks: [f32; 3],
    pub tv: [f32; 4],
    pub neon: [f32; 4],
    pub rocket: [f32; 2],
    pub coffee: [f32; 3],
    pub aq: [f32; 2],
    pub arc: [f32; 2],
    pub prn: [f32; 2],
    pub lost: [f32; 3],
    pub sofa: [f32; 4],
    pub game_tv: Option<[f32; 2]>,
    pub bil: Option<[f32; 2]>,
    pub ktv: Option<[f32; 3]>,
    pub meet: [f32; 2],
    pub pods: Vec<[f32; 2]>,
    pub depts: Vec<DeptRoom>,
    pub desks: Vec<DeskCoord>,
}

impl LayoutSpec {
    pub fn for_layout(layout: OfficeLayout) -> Self {
        let depts: Vec<DeptRoom> = if layout.is_hq() {
            [
                (-8.0_f32, 2.3_f32, 10.8_f32, 18.4_f32, -2.85_f32),
                (4.3, 14.0, 10.8, 18.4, 9.15),
                (-8.0, 2.3, 18.4, 26.0, -3.05),
                (4.3, 14.0, 18.4, 26.0, 9.5),
            ]
            .into_iter()
            .map(|(x0, x1, z0, z1, cx)| {
                let cz = (z0 + z1) * 0.5;
                DeptRoom {
                    x0,
                    x1,
                    z0,
                    z1,
                    cx,
                    cz,
                    pods: [[cx - 2.65, cz], [cx + 2.65, cz]],
                }
            })
            .collect()
        } else {
            Vec::new()
        };

        let mut pods = Vec::new();
        let mut desks = Vec::new();
        if layout.is_pro() {
            pods.extend_from_slice(&[[-0.1, 1.5], [5.1, 1.5], [2.5, 6.6]]);
            for d in &depts {
                pods.extend_from_slice(&d.pods);
            }
            for &[cx, cz] in &pods {
                for &(sx, sz) in &[(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    desks.push(DeskCoord {
                        x: cx + sx * 0.76,
                        z: cz + sz * 0.375,
                        ry: if sz > 0.0 { PI } else { 0.0 },
                    });
                }
            }
        } else {
            for &z in &[-1.6_f32, 0.4, 2.4] {
                for &x in &[-2.1_f32, 0.0, 2.1] {
                    desks.push(DeskCoord { x, z, ry: 0.0 });
                }
            }
        }

        if layout.is_pro() {
            let hq = layout.is_hq();
            Self {
                layout,
                bounds: FloorRect {
                    x0: -8.0,
                    x1: 14.0,
                    z0: -8.0,
                    z1: if hq { 26.0 } else { 9.0 },
                },
                reach: if hq {
                    FloorRect { x0: -11.0, x1: 16.0, z0: -9.0, z1: 29.5 }
                } else {
                    FloorRect { x0: -11.0, x1: 14.5, z0: -9.0, z1: 13.5 }
                },
                center: if hq { Vec2::new(3.0, 9.0) } else { Vec2::new(3.0, 0.5) },
                home: if hq { Vec3::new(3.0, 0.3, 9.0) } else { Vec3::new(3.0, 0.3, 0.7) },
                cam_offset: if hq { Vec3::new(22.0, 25.0, 22.0) } else { Vec3::new(17.0, 20.0, 17.0) },
                viewport_height: if hq { 27.5 } else { 19.5 },
                shelf: [4.95, -7.62, 0.0],
                globe: [8.0, -6.55],
                board: [-3.42, 2.05, 7.25, 0.0],
                door_z: 3.3,
                clock: [-7.96, 2.15, 1.7, PI * 0.5],
                racks: [-7.55, -1.47, PI * 0.5],
                tv: [-7.96, 3.35, -1.47, PI * 0.5],
                neon: [0.2, 3.62, -7.97, 0.0],
                rocket: [13.15, 1.75],
                coffee: [10.6, -7.45, PI],
                aq: [10.45, 8.45],
                arc: [13.4, 8.45],
                prn: [-3.15, 0.55],
                lost: [-5.2, -0.62, PI * 0.5 - 0.5],
                sofa: [11.0, 3.95, 2.2, PI],
                game_tv: Some([11.0, 0.9]),
                bil: Some([12.85, 6.2]),
                ktv: Some([-0.75, 7.4, PI * 0.5]),
                meet: [0.2, -5.6],
                pods,
                depts,
                desks,
            }
        } else {
            Self {
                layout,
                bounds: FloorRect { x0: -8.0, x1: 8.0, z0: -6.0, z1: 6.0 },
                reach: FloorRect { x0: -11.0, x1: 14.5, z0: -9.0, z1: 13.5 },
                center: Vec2::ZERO,
                home: Vec3::new(0.0, 0.3, 0.2),
                cam_offset: Vec3::new(18.0, 16.0, 18.0),
                viewport_height: 16.0,
                shelf: [-5.2, -5.62, 0.0],
                globe: [0.6, -5.0],
                board: [-7.94, 2.05, -2.6, 0.0],
                door_z: 1.5,
                clock: [-2.45, 3.05, -5.96, 0.0],
                racks: [5.5, -5.5, 0.0],
                tv: [5.55, 3.3, -5.96, 0.0],
                neon: [-5.2, 3.62, -5.97, 0.0],
                rocket: [7.25, -3.6],
                coffee: [-5.55, 5.3, 0.0],
                aq: [1.35, 5.3],
                arc: [-1.45, 5.42],
                prn: [-3.15, 5.4],
                lost: [4.18, -4.95, 0.5],
                sofa: [-2.15, -5.5, 2.1, 0.0],
                game_tv: None,
                bil: None,
                ktv: None,
                meet: [5.2, 2.4],
                pods,
                depts,
                desks,
            }
        }
    }

    pub fn inside(&self, x: f32, z: f32, margin: f32) -> bool {
        x > self.bounds.x0 + margin
            && x < self.bounds.x1 - margin
            && z > self.bounds.z0 + margin
            && z < self.bounds.z1 - margin
    }
}
