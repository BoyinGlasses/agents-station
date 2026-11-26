use crate::layout::{LayoutSpec, OfficeLayout};
use bevy::prelude::*;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// A solid 2D wall segment in the office XZ plane (`a -> b`) that walking paths must never cross.
#[derive(Debug, Clone, Copy)]
pub struct WallSegment {
    pub a: Vec2,
    pub b: Vec2,
}

/// A freshly mopped floor strip (`staff.js`); routes avoid crossing it unless no dry detour exists.
#[derive(Debug, Clone, Copy)]
pub struct WetStrip {
    pub a: Vec2,
    pub b: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StationKind {
    Desk,
    Library,
    Board,
    Globe,
    Server,
    Coffee,
    Meeting,
    Director,
    Wc,
    Sink,
    Dining,
}

#[derive(Debug, Clone, Copy)]
pub struct NavSpot {
    pub pos: Vec3,
    pub face: f32,
    pub node_idx: usize,
    pub sit: bool,
}

#[derive(Debug, Clone)]
struct NavNode {
    name: String,
    pos: Vec2,
    is_doorway: bool,
}

/// Room-door walking graph ported 1:1 from `src/office/people/navigation.js` (`NODES` / `ADJ`)
/// with solid partition wall segments matching `pro_floor.rs` & `hq_wing.rs`.
#[derive(Resource, Debug, Clone)]
pub struct NavGraph {
    layout: OfficeLayout,
    nodes: Vec<NavNode>,
    index_by_name: HashMap<String, usize>,
    adj: Vec<Vec<usize>>,
    walls: Vec<WallSegment>,
    wet_strips: Vec<WetStrip>,
    desk_spots: Vec<NavSpot>,
    stations: HashMap<StationKind, Vec<NavSpot>>,
    door_in: NavSpot,
    door_out: NavSpot,
}

#[derive(Clone, Copy)]
struct HeapState {
    cost: f32,
    node: usize,
}

impl PartialEq for HeapState {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.cost.to_bits() == other.cost.to_bits()
    }
}

impl Eq for HeapState {}

impl Ord for HeapState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| self.node.cmp(&other.node))
    }
}

impl PartialOrd for HeapState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn face_to(x: f32, z: f32, tx: f32, tz: f32) -> f32 {
    (tx - x).atan2(tz - z)
}

/// Exact 2D segment intersection check (including collinear overlap and endpoint touch).
pub fn segments_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    let cross = |o: Vec2, p: Vec2, q: Vec2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let on_seg = |p: Vec2, q: Vec2, r: Vec2| {
        q.x <= p.x.max(r.x) + 1e-4
            && q.x >= p.x.min(r.x) - 1e-4
            && q.y <= p.y.max(r.y) + 1e-4
            && q.y >= p.y.min(r.y) - 1e-4
    };
    let d1 = cross(a1, a2, b1);
    let d2 = cross(a1, a2, b2);
    let d3 = cross(b1, b2, a1);
    let d4 = cross(b1, b2, a2);

    if ((d1 > 1e-5 && d2 < -1e-5) || (d1 < -1e-5 && d2 > 1e-5))
        && ((d3 > 1e-5 && d4 < -1e-5) || (d3 < -1e-5 && d4 > 1e-5))
    {
        return true;
    }
    if d1.abs() <= 1e-5 && on_seg(a1, b1, a2) {
        return true;
    }
    if d2.abs() <= 1e-5 && on_seg(a1, b2, a2) {
        return true;
    }
    if d3.abs() <= 1e-5 && on_seg(b1, a1, b2) {
        return true;
    }
    if d4.abs() <= 1e-5 && on_seg(b1, a2, b2) {
        return true;
    }
    false
}

fn push_wall_with_gaps(
    walls: &mut Vec<WallSegment>,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
    gaps: &[(f32, f32)],
) {
    let a = Vec2::new(ax, az);
    let b = Vec2::new(bx, bz);
    let delta = b - a;
    let len = delta.length();
    if len < 0.05 {
        return;
    }
    let dir = delta / len;
    let mut t = 0.0_f32;
    for &(g0, g1) in gaps {
        if g0 - t > 0.04 {
            walls.push(WallSegment {
                a: a + dir * t,
                b: a + dir * g0,
            });
        }
        t = g1;
    }
    if len - t > 0.04 {
        walls.push(WallSegment {
            a: a + dir * t,
            b: a + dir * len,
        });
    }
}

fn point_to_segment_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let denom = ab.length_squared();
    if denom <= 1e-6 {
        return p.distance(a);
    }
    let k = ((p - a).dot(ab) / denom).clamp(0.0, 1.0);
    p.distance(a + ab * k)
}

fn build_partition_walls(layout: OfficeLayout) -> Vec<WallSegment> {
    if !layout.is_pro() {
        return Vec::new();
    }
    let mut walls = Vec::with_capacity(32);
    // Exact interior glass & cutaway walls from `pro_floor.rs:334-346`
    push_wall_with_gaps(&mut walls, -8.0, -3.0, -2.6, -3.0, &[(4.5, 5.3)]);
    push_wall_with_gaps(&mut walls, -2.6, -8.0, -2.6, -3.0, &[]);
    push_wall_with_gaps(&mut walls, -2.6, -3.0, 3.0, -3.0, &[(4.5, 5.4)]);
    push_wall_with_gaps(&mut walls, 3.0, -8.0, 3.0, -3.0, &[]);
    push_wall_with_gaps(&mut walls, 3.0, -3.0, 8.6, -3.0, &[(0.3, 1.3)]);
    push_wall_with_gaps(&mut walls, 8.6, -8.0, 8.6, -3.0, &[]);
    push_wall_with_gaps(&mut walls, -3.6, -3.0, -3.6, 0.55, &[(1.0, 1.9)]);
    push_wall_with_gaps(&mut walls, -8.0, 0.55, -3.6, 0.55, &[]);
    push_wall_with_gaps(&mut walls, -8.0, 5.4, -3.6, 5.4, &[(2.6, 3.6)]);
    push_wall_with_gaps(&mut walls, -3.6, 5.4, -3.6, 9.0, &[]);
    push_wall_with_gaps(&mut walls, -3.6, 0.6, -3.6, 5.33, &[(2.1, 3.3)]);
    push_wall_with_gaps(&mut walls, 8.6, -1.6, 8.6, 9.0, &[(5.8, 8.0)]);
    push_wall_with_gaps(&mut walls, 8.6, 0.6, 14.0, 0.6, &[(0.3, 1.3)]);

    if layout.is_hq() {
        // Exact HQ wing walls from `hq_wing.rs:212-239`
        let wing_z0 = 9.0_f32;
        let wing_z1 = 26.0_f32;
        let wing_split = 18.4_f32;
        let [dw, de] = [-2.4_f32, 7.4_f32];
        push_wall_with_gaps(&mut walls, -8.0, wing_z0, -3.6, wing_z0, &[]);
        push_wall_with_gaps(
            &mut walls,
            -3.6,
            wing_z0,
            8.6,
            wing_z0,
            &[(dw - 0.6 + 3.6, dw + 0.6 + 3.6), (de - 0.6 + 3.6, de + 0.6 + 3.6)],
        );
        push_wall_with_gaps(&mut walls, 8.6, wing_z0, 14.0, wing_z0, &[]);

        push_wall_with_gaps(
            &mut walls,
            -8.0,
            10.8,
            2.3,
            10.8,
            &[(-2.4 - 0.6 + 8.0, -2.4 + 0.6 + 8.0)],
        );
        push_wall_with_gaps(
            &mut walls,
            4.3,
            10.8,
            14.0,
            10.8,
            &[(9.15 - 0.6 - 4.3, 9.15 + 0.6 - 4.3)],
        );

        let door_b = 22.2 - 10.8;
        push_wall_with_gaps(&mut walls, 2.3, 10.8, 2.3, wing_z1, &[(door_b - 0.6, door_b + 0.6)]);
        push_wall_with_gaps(&mut walls, 4.3, 10.8, 4.3, wing_z1, &[(door_b - 0.6, door_b + 0.6)]);

        push_wall_with_gaps(&mut walls, -8.0, wing_split, 2.3, wing_split, &[]);
        push_wall_with_gaps(&mut walls, 4.3, wing_split, 14.0, wing_split, &[]);
    }

    walls
}

impl NavGraph {
    pub fn for_layout(layout: OfficeLayout) -> Self {
        let spec = LayoutSpec::for_layout(layout);
        let walls = build_partition_walls(layout);

        let mut g = Self {
            layout,
            nodes: Vec::new(),
            index_by_name: HashMap::new(),
            adj: Vec::new(),
            walls,
            wet_strips: Vec::new(),
            desk_spots: Vec::new(),
            stations: HashMap::new(),
            door_in: NavSpot {
                pos: Vec3::ZERO,
                face: 0.0,
                node_idx: 0,
                sit: false,
            },
            door_out: NavSpot {
                pos: Vec3::ZERO,
                face: 0.0,
                node_idx: 0,
                sit: false,
            },
        };

        if layout.is_pro() {
            g.build_pro_and_hq(&spec);
        } else {
            g.build_cozy(&spec);
        }
        g
    }

    fn add_node(&mut self, name: impl Into<String>, x: f32, z: f32, is_doorway: bool) -> usize {
        let key = name.into();
        if let Some(&idx) = self.index_by_name.get(&key) {
            self.nodes[idx].pos = Vec2::new(x, z);
            self.nodes[idx].is_doorway |= is_doorway;
            return idx;
        }
        let idx = self.nodes.len();
        self.nodes.push(NavNode {
            name: key.clone(),
            pos: Vec2::new(x, z),
            is_doorway,
        });
        self.index_by_name.insert(key, idx);
        self.adj.push(Vec::new());
        idx
    }

    fn add_edge(&mut self, a: &str, b: &str) {
        let (Some(&ia), Some(&ib)) = (self.index_by_name.get(a), self.index_by_name.get(b)) else {
            return;
        };
        if !self.adj[ia].contains(&ib) {
            self.adj[ia].push(ib);
        }
        if !self.adj[ib].contains(&ia) {
            self.adj[ib].push(ia);
        }
    }

    fn spot(&self, x: f32, z: f32, face: f32, node_name: &str, sit: bool) -> NavSpot {
        let node_idx = self
            .index_by_name
            .get(node_name)
            .copied()
            .unwrap_or_else(|| self.nearest_node(Vec3::new(x, 0.0, z)));
        NavSpot {
            pos: Vec3::new(x, 0.0, z),
            face,
            node_idx,
            sit,
        }
    }

    fn build_cozy(&mut self, spec: &LayoutSpec) {
        let nodes: [(&str, f32, f32, bool); 24] = [
            ("A", -5.2, -3.3, false),
            ("BL", -3.4, -3.15, false),
            ("T0", -2.1, -3.15, false),
            ("T1", 0.0, -3.2, false),
            ("T2", 2.1, -3.15, false),
            ("D", 3.4, -3.1, false),
            ("E", 5.6, -3.2, false),
            ("F", -6.3, -0.9, false),
            ("GL1", -3.4, -0.9, false),
            ("U0", -2.1, -0.9, false),
            ("U1", 0.0, -0.9, false),
            ("U2", 2.1, -0.9, false),
            ("GR1", 3.4, -0.9, false),
            ("P", 6.9, 0.6, false),
            ("I", -6.5, 1.5, true),
            ("GL2", -3.4, 1.1, false),
            ("V0", -2.1, 1.1, false),
            ("V1", 0.0, 1.1, false),
            ("V2", 2.1, 1.1, false),
            ("GR2", 3.4, 1.1, false),
            ("J", -5.5, 3.6, false),
            ("K", -3.4, 3.6, false),
            ("L", 0.0, 3.7, false),
            ("M", 3.0, 4.1, false),
        ];
        for (name, x, z, door) in nodes {
            self.add_node(name, x, z, door);
        }

        let edges = [
            ("A", "BL"),
            ("BL", "T0"),
            ("T0", "T1"),
            ("T1", "T2"),
            ("T2", "D"),
            ("D", "E"),
            ("A", "F"),
            ("F", "I"),
            ("F", "GL1"),
            ("BL", "GL1"),
            ("GL1", "U0"),
            ("U0", "U1"),
            ("U1", "U2"),
            ("U2", "GR1"),
            ("GR1", "D"),
            ("GL1", "GL2"),
            ("I", "GL2"),
            ("GL2", "V0"),
            ("V0", "V1"),
            ("V1", "V2"),
            ("V2", "GR2"),
            ("GR1", "GR2"),
            ("GR1", "P"),
            ("P", "E"),
            ("I", "J"),
            ("J", "K"),
            ("K", "GL2"),
            ("K", "L"),
            ("L", "M"),
            ("M", "GR2"),
        ];
        for (a, b) in edges {
            self.add_edge(a, b);
        }

        const ROW_NODES: [[&str; 3]; 3] = [
            ["T0", "T1", "T2"],
            ["U0", "U1", "U2"],
            ["V0", "V1", "V2"],
        ];
        for (i, d) in spec.desks.iter().enumerate() {
            let rot = Quat::from_rotation_y(d.ry);
            let p = Vec3::new(d.x, 0.0, d.z) + rot * Vec3::new(0.0, 0.0, -0.66);
            let node = ROW_NODES[(i / 3).min(2)][i % 3];
            self.desk_spots.push(self.spot(p.x, p.z, d.ry, node, true));
        }

        let dz = spec.door_z;
        self.door_in = self.spot(-7.2, dz, FRAC_PI_2, "I", false);
        self.door_out = self.spot(-8.9, dz, -FRAC_PI_2, "I", false);

        self.stations.insert(
            StationKind::Library,
            vec![
                self.spot(-6.3, -4.45, PI, "A", false),
                self.spot(-4.1, -4.45, PI, "A", false),
                self.spot(-5.2, -4.35, PI, "A", false),
            ],
        );
        self.stations.insert(
            StationKind::Board,
            vec![
                self.spot(-6.95, -3.25, -FRAC_PI_2, "F", false),
                self.spot(-6.95, -1.95, -FRAC_PI_2, "F", false),
            ],
        );
        self.stations.insert(
            StationKind::Globe,
            vec![
                self.spot(1.7, -4.4, face_to(1.7, -4.4, 0.6, -5.0), "T1", false),
                self.spot(-0.5, -4.4, face_to(-0.5, -4.4, 0.6, -5.0), "T1", false),
            ],
        );
        self.stations.insert(
            StationKind::Server,
            vec![
                self.spot(4.5, -4.25, PI, "E", false),
                self.spot(6.6, -4.25, PI, "E", false),
                self.spot(5.55, -4.15, PI, "E", false),
            ],
        );
        self.stations.insert(
            StationKind::Coffee,
            vec![
                self.spot(-6.5, 4.4, 0.0, "J", false),
                self.spot(-4.5, 4.4, 0.0, "J", false),
                self.spot(-5.5, 4.45, 0.0, "J", false),
            ],
        );
        let mx = spec.meet[0];
        let mz = spec.meet[1];
        let mut meet = Vec::new();
        for k in 0..4 {
            let a = FRAC_PI_4 + (k as f32) * FRAC_PI_2;
            let x = mx + a.sin() * 1.35;
            let z = mz + a.cos() * 1.35;
            let n = if z > mz { "M" } else if x > mx { "P" } else { "GR2" };
            meet.push(self.spot(x, z, face_to(x, z, mx, mz), n, true));
        }
        self.stations.insert(StationKind::Meeting, meet);
    }

    fn build_pro_and_hq(&mut self, spec: &LayoutSpec) {
        let pro_nodes: [(&str, f32, f32, bool); 52] = [
            // Lobby & entrance
            ("O", -7.5, 3.3, true),
            ("I", -6.3, 3.3, true),
            ("LB", -5.0, 3.3, true),
            // Server room doorway
            ("SVo", -3.1, -1.55, true),
            ("SVi", -4.2, -1.55, true),
            // Restroom doorway & stalls
            ("WCo", -4.9, 4.7, true),
            ("WCi", -4.9, 6.2, true),
            ("ST1", -6.05, 6.1, false),
            ("ST2", -6.05, 7.4, false),
            ("WCs", -6.05, 8.5, false),
            // Director's office doorway & room
            ("L0", -3.1, -2.3, true),
            ("DRi", -3.1, -3.9, true),
            ("DRc", -5.4, -3.9, false),
            ("DRs", -3.85, -6.7, false),
            ("DRw", -4.05, -4.45, false),
            // Meeting room doorway & loop
            ("MTd", 2.35, -2.3, true),
            ("MTi", 2.35, -3.75, true),
            ("MF2", 1.35, -3.75, false),
            ("MF1", 0.2, -3.75, false),
            ("MF0", -0.95, -3.75, false),
            ("MTf", -2.05, -3.75, false),
            ("MTw", -2.05, -7.45, false),
            ("MB0", -0.95, -7.45, false),
            ("MB1", 0.2, -7.45, false),
            ("MB2", 1.35, -7.45, false),
            ("MTb", 2.5, -7.45, false),
            // Library doorway & room
            ("LBd", 3.8, -2.3, true),
            ("LBi", 3.8, -3.8, true),
            ("LBf", 6.25, -3.45, false),
            ("LBm", 4.6, -5.6, false),
            ("CE", 7.4, -2.3, true),
            // Dev core vertical aisles
            ("L1", -2.4, -0.3, false),
            ("L2", -2.4, 3.3, true),
            ("L3", -2.4, 4.8, false),
            ("L4", -2.4, 8.4, spec.layout.is_hq()),
            ("R1", 7.4, -0.3, false),
            ("R2", 7.4, 3.3, false),
            ("R3", 7.4, 4.8, true),
            ("R4", 7.4, 8.4, spec.layout.is_hq()),
            ("M1", 2.5, -0.3, false),
            ("M2", 2.5, 3.3, false),
            // Pantry
            ("PA1", 9.3, -2.6, true),
            ("PA2", 9.3, -6.15, false),
            ("PA3", 11.7, -2.6, false),
            ("PA4", 11.7, -6.15, false),
            ("PA5", 9.3, 0.45, true),
            ("PA6", 11.7, 0.45, false),
            // Lounge & karaoke
            ("LG1", 9.3, 1.6, true),
            ("LG2", 12.2, 1.5, false),
            ("LGs", 9.5, 3.7, false),
            ("LGr", 12.75, 3.9, false),
            ("LG3", 9.0, 6.0, true),
        ];
        for (name, x, z, door) in pro_nodes {
            self.add_node(name, x, z, door);
        }
        self.add_node("BK", 0.32, 8.4, false);

        // Pod aisle nodes behind every pair of chairs (`navigation.js:14, 186`)
        for (p, &[cx, cz]) in spec.pods.iter().enumerate() {
            for k in 0..2 {
                let dx = if k == 0 { -0.76 } else { 0.76 };
                self.add_node(format!("P{p}b{k}"), cx + dx, cz - 1.8, false);
                self.add_node(format!("P{p}f{k}"), cx + dx, cz + 1.8, false);
            }
        }

        let pro_edges = [
            ("O", "I"),
            ("I", "LB"),
            ("L0", "SVo"),
            ("L1", "SVo"),
            ("SVo", "SVi"),
            ("LB", "WCo"),
            ("WCo", "WCi"),
            ("WCi", "ST1"),
            ("WCi", "ST2"),
            ("WCi", "WCs"),
            ("ST1", "ST2"),
            ("LB", "L2"),
            ("L0", "L1"),
            ("L1", "L2"),
            ("L2", "L3"),
            ("L3", "L4"),
            ("R1", "R2"),
            ("R2", "R3"),
            ("R3", "R4"),
            ("M1", "M2"),
            ("L1", "P0b0"),
            ("P0b0", "P0b1"),
            ("P0b1", "M1"),
            ("M1", "P1b0"),
            ("P1b0", "P1b1"),
            ("P1b1", "R1"),
            ("L2", "P0f0"),
            ("P0f0", "P0f1"),
            ("P0f1", "M2"),
            ("M2", "P1f0"),
            ("P1f0", "P1f1"),
            ("P1f1", "R2"),
            ("L3", "P2b0"),
            ("P2b0", "P2b1"),
            ("P2b1", "R3"),
            ("L4", "BK"),
            ("BK", "P2f0"),
            ("P2f0", "P2f1"),
            ("P2f1", "R4"),
            ("L0", "DRi"),
            ("DRi", "DRc"),
            ("DRi", "DRw"),
            ("DRw", "DRs"),
            ("L0", "MTd"),
            ("MTd", "MTi"),
            ("MTi", "MF2"),
            ("MF2", "MF1"),
            ("MF1", "MF0"),
            ("MF0", "MTf"),
            ("MTf", "MTw"),
            ("MTw", "MB0"),
            ("MB0", "MB1"),
            ("MB1", "MB2"),
            ("MB2", "MTb"),
            ("MTb", "MTi"),
            ("MTd", "LBd"),
            ("LBd", "LBi"),
            ("LBi", "LBf"),
            ("LBi", "LBm"),
            ("LBd", "CE"),
            ("CE", "R1"),
            ("CE", "PA1"),
            ("PA1", "PA2"),
            ("PA1", "PA3"),
            ("PA2", "PA4"),
            ("PA3", "PA4"),
            ("PA1", "PA5"),
            ("PA3", "PA6"),
            ("PA5", "PA6"),
            ("PA5", "LG1"),
            ("LG1", "LG2"),
            ("LG1", "LGs"),
            ("LGs", "LG3"),
            ("LG3", "R3"),
            ("LG2", "LGr"),
        ];
        for (a, b) in pro_edges {
            self.add_edge(a, b);
        }

        // Headquarters wing graph (`navigation.js:68-86`)
        if spec.layout.is_hq() {
            self.add_node("HAL", -2.4, 9.9, true);
            self.add_node("HB0", 3.3, 9.9, false);
            self.add_node("HAR", 7.4, 9.9, true);
            self.add_node("HB1", 3.3, 22.2, true);
            for (a, b) in [
                ("L4", "HAL"),
                ("R4", "HAR"),
                ("HAL", "HB0"),
                ("HB0", "HAR"),
                ("HB0", "HB1"),
            ] {
                self.add_edge(a, b);
            }

            // Exact door metadata matching `my-station/src/office/world/layout.js:10-15`
            let doors: [(char, f32); 4] = [('n', -2.4), ('n', 9.15), ('e', 22.2), ('w', 22.2)];
            for (i, d) in spec.depts.iter().enumerate() {
                let (side, at) = doors[i];
                let pl = 3 + i * 2;
                let pr = pl + 1;
                let zb = d.cz - 1.8;
                let zf = d.cz + 1.8;
                let lane = if side == 'n' { at } else { d.cx };
                let db = format!("D{i}b");
                let df = format!("D{i}f");
                self.add_node(db.clone(), lane, zb, false);
                self.add_node(df.clone(), lane, zf, false);

                let plb0 = format!("P{pl}b0");
                let plb1 = format!("P{pl}b1");
                let prb0 = format!("P{pr}b0");
                let prb1 = format!("P{pr}b1");
                let plf0 = format!("P{pl}f0");
                let plf1 = format!("P{pl}f1");
                let prf0 = format!("P{pr}f0");
                let prf1 = format!("P{pr}f1");

                self.add_edge(&db, &df);
                self.add_edge(&db, &plb1);
                self.add_edge(&plb1, &plb0);
                self.add_edge(&db, &prb0);
                self.add_edge(&prb0, &prb1);
                self.add_edge(&df, &plf1);
                self.add_edge(&plf1, &plf0);
                self.add_edge(&df, &prf0);
                self.add_edge(&prf0, &prf1);

                let di = format!("D{i}i");
                if side == 'n' {
                    self.add_node(di.clone(), lane, d.z0 + 1.0, true);
                    if (lane - (-2.4)).abs() > 0.05 {
                        let hai = format!("HA{i}");
                        self.add_node(hai.clone(), lane, 9.9, true);
                        self.add_edge("HAR", &hai);
                        self.add_edge(&hai, &di);
                    } else {
                        self.add_edge("HAL", &di);
                    }
                    self.add_edge(&di, &db);
                } else {
                    let x = if side == 'e' { d.x1 - 0.4 } else { d.x0 + 0.5 };
                    let near = if side == 'e' { pr } else { pl };
                    let k = if side == 'e' { 1 } else { 0 };
                    let dsb = format!("D{i}sb");
                    let dsf = format!("D{i}sf");
                    self.add_node(di.clone(), x, at, true);
                    self.add_node(dsb.clone(), x, zb, false);
                    self.add_node(dsf.clone(), x, zf, false);
                    self.add_edge("HB1", &di);
                    self.add_edge(&di, &dsb);
                    self.add_edge(&di, &dsf);
                    self.add_edge(&dsb, &format!("P{near}b{k}"));
                    self.add_edge(&dsf, &format!("P{near}f{k}"));
                }
            }
        }

        // Desk chair spots (`navigation.js:218-223`)
        for (i, d) in spec.desks.iter().enumerate() {
            let rot = Quat::from_rotation_y(d.ry);
            let p = Vec3::new(d.x, 0.0, d.z) + rot * Vec3::new(0.0, 0.0, -0.66);
            let pod = i / 4;
            let k = i % 4;
            let side = if k < 2 { 'b' } else { 'f' };
            let node_name = format!("P{pod}{side}{}", k % 2);
            self.desk_spots.push(self.spot(p.x, p.z, d.ry, &node_name, true));
        }

        let dz = spec.door_z;
        self.door_in = self.spot(-6.3, dz, -FRAC_PI_2, "I", false);
        self.door_out = self.spot(-7.5, dz, FRAC_PI_2, "O", false);

        // Station spots (`navigation.js:230-243`)
        self.stations.insert(
            StationKind::Library,
            vec![
                self.spot(5.6, -4.1, 0.0, "LBf", true),
                self.spot(6.9, -4.1, 0.0, "LBf", true),
                self.spot(4.0, -6.95, PI, "LBm", false),
                self.spot(5.0, -6.95, PI, "LBm", false),
                self.spot(6.0, -6.95, PI, "LBm", false),
            ],
        );
        self.stations.insert(
            StationKind::Board,
            vec![
                self.spot(-2.75, 6.55, -FRAC_PI_2, "L3", false),
                self.spot(-2.75, 7.95, -FRAC_PI_2, "L4", false),
            ],
        );
        self.stations.insert(
            StationKind::Globe,
            vec![
                self.spot(7.1, -6.2, face_to(7.1, -6.2, 8.0, -6.55), "LBm", false),
                self.spot(7.6, -5.3, face_to(7.6, -5.3, 8.0, -6.55), "LBm", false),
            ],
        );
        self.stations.insert(
            StationKind::Server,
            vec![
                self.spot(-6.55, -2.52, -FRAC_PI_2, "SVi", false),
                self.spot(-6.55, -1.47, -FRAC_PI_2, "SVi", false),
                self.spot(-6.55, -0.42, -FRAC_PI_2, "SVi", false),
            ],
        );
        let mut dining = Vec::with_capacity(16);
        for (ti, (tx, tz)) in [
            (10.4_f32, -4.9_f32),
            (12.8, -4.9),
            (10.4, -0.9),
            (12.8, -0.9),
        ]
        .into_iter()
        .enumerate()
        {
            for sz in [-1.0_f32, 1.0] {
                for sx in [-1.0_f32, 1.0] {
                    let x = tx + sx * 0.33;
                    let z = tz + sz * 0.62;
                    let near = if x < 11.05 { 0 } else { 1 };
                    let node = if sz < 0.0 {
                        if ti < 2 {
                            ["PA2", "PA4"][near]
                        } else {
                            ["PA1", "PA3"][near]
                        }
                    } else if ti < 2 {
                        ["PA1", "PA3"][near]
                    } else {
                        ["PA5", "PA6"][near]
                    };
                    dining.push(self.spot(x, z, if sz < 0.0 { 0.0 } else { PI }, node, true));
                }
            }
        }
        let mut coffee = vec![
            self.spot(11.45, -6.75, PI, "PA4", false),
            self.spot(10.35, -6.75, PI, "PA2", false),
            self.spot(10.5, 3.8, PI, "LGs", true),
            self.spot(11.5, 3.8, PI, "LGr", true),
        ];
        coffee.extend_from_slice(&dining);
        self.stations.insert(StationKind::Dining, dining);
        self.stations.insert(StationKind::Coffee, coffee);
        self.stations.insert(
            StationKind::Meeting,
            vec![
                self.spot(-0.95, -4.65, PI, "MF0", true),
                self.spot(0.2, -4.65, PI, "MF1", true),
                self.spot(1.35, -4.65, PI, "MF2", true),
                self.spot(-0.95, -6.55, 0.0, "MB0", true),
                self.spot(0.2, -6.55, 0.0, "MB1", true),
                self.spot(1.35, -6.55, 0.0, "MB2", true),
                self.spot(2.55, -4.7, -FRAC_PI_2, "MTi", false),
                self.spot(2.6, -5.6, -FRAC_PI_2, "MTb", false),
                self.spot(-2.2, -6.6, FRAC_PI_2, "MTw", false),
                self.spot(-2.2, -4.5, FRAC_PI_2, "MTf", false),
            ],
        );
        self.stations.insert(
            StationKind::Director,
            vec![
                self.spot(-5.95, -4.95, PI, "DRc", true),
                self.spot(-4.85, -4.95, PI, "DRc", true),
            ],
        );
        self.stations.insert(
            StationKind::Wc,
            vec![
                self.spot(-7.3, 6.1, FRAC_PI_2, "ST1", false),
                self.spot(-7.3, 7.4, FRAC_PI_2, "ST2", false),
            ],
        );
        self.stations.insert(
            StationKind::Sink,
            vec![self.spot(-7.25, 8.5, -FRAC_PI_2, "WCs", false)],
        );
    }

    pub fn layout(&self) -> OfficeLayout {
        self.layout
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.adj.iter().map(|v| v.len()).sum::<usize>() / 2
    }

    pub fn node_index(&self, name: &str) -> Option<usize> {
        self.index_by_name.get(name).copied()
    }

    pub fn node_pos(&self, name: &str) -> Option<Vec3> {
        let idx = *self.index_by_name.get(name)?;
        let p = self.nodes[idx].pos;
        Some(Vec3::new(p.x, 0.0, p.y))
    }

    pub fn node_pos_by_index(&self, idx: usize) -> Option<Vec3> {
        let p = self.nodes.get(idx)?.pos;
        Some(Vec3::new(p.x, 0.0, p.y))
    }

    pub fn doorway_nodes(&self) -> Vec<(&str, Vec3)> {
        self.nodes
            .iter()
            .filter(|n| n.is_doorway)
            .map(|n| (n.name.as_str(), Vec3::new(n.pos.x, 0.0, n.pos.y)))
            .collect()
    }

    pub fn walls(&self) -> &[WallSegment] {
        &self.walls
    }

    pub fn crosses_wall(&self, a: Vec3, b: Vec3) -> bool {
        let a2 = Vec2::new(a.x, a.z);
        let b2 = Vec2::new(b.x, b.z);
        self.walls
            .iter()
            .any(|w| segments_intersect(a2, b2, w.a, w.b))
    }

    /// Finds the nearest node with an unobstructed line-of-sight (never snapping across a partition wall).
    pub fn nearest_node(&self, pos: Vec3) -> usize {
        let p = Vec2::new(pos.x, pos.z);
        let mut best_clear: Option<(usize, f32)> = None;
        let mut best_any: Option<(usize, f32)> = None;

        for (i, n) in self.nodes.iter().enumerate() {
            let d = p.distance_squared(n.pos);
            if best_any.is_none_or(|(_, bd)| d < bd) {
                best_any = Some((i, d));
            }
            let blocked = self
                .walls
                .iter()
                .any(|w| segments_intersect(p, n.pos, w.a, w.b));
            if !blocked && best_clear.is_none_or(|(_, bd)| d < bd) {
                best_clear = Some((i, d));
            }
        }
        best_clear
            .or(best_any)
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    fn wet_edge_cost(&self, a: Vec2, b: Vec2) -> f32 {
        if self.wet_strips.is_empty() {
            return 0.0;
        }
        for w in &self.wet_strips {
            for i in 0..=4 {
                let t = i as f32 * 0.25;
                let sample = a.lerp(b, t);
                if point_to_segment_dist(sample, w.a, w.b) < 0.6 {
                    return 30.0;
                }
            }
        }
        0.0
    }

    /// Dijkstra shortest path over node indices (`navigation.js::route`).
    pub fn route_nodes(&self, start: usize, goal: usize) -> Vec<usize> {
        let n = self.nodes.len();
        if n == 0 || start >= n || goal >= n {
            return Vec::new();
        }
        if start == goal {
            return vec![start];
        }
        let mut dist = vec![f32::INFINITY; n];
        let mut prev = vec![usize::MAX; n];
        let mut heap = BinaryHeap::new();

        dist[start] = 0.0;
        heap.push(HeapState {
            cost: 0.0,
            node: start,
        });

        while let Some(HeapState { cost, node }) = heap.pop() {
            if node == goal {
                break;
            }
            if cost > dist[node] + 1e-5 {
                continue;
            }
            let p_a = self.nodes[node].pos;
            for &next in &self.adj[node] {
                let p_b = self.nodes[next].pos;
                let step = p_a.distance(p_b) + self.wet_edge_cost(p_a, p_b);
                let next_cost = cost + step;
                if next_cost < dist[next] {
                    dist[next] = next_cost;
                    prev[next] = node;
                    heap.push(HeapState {
                        cost: next_cost,
                        node: next,
                    });
                }
            }
        }

        if prev[goal] == usize::MAX {
            return vec![start, goal];
        }
        let mut out = Vec::new();
        let mut cur = goal;
        while cur != usize::MAX {
            out.push(cur);
            if cur == start {
                break;
            }
            cur = prev[cur];
        }
        out.reverse();
        out
    }

    /// Computes a wall-safe 3D waypoint path from `from` to `to` through `NODES`/`ADJ` doorways.
    pub fn shortest_path(&self, from: Vec3, to: Vec3) -> Vec<Vec3> {
        let to_flat = Vec3::new(to.x, 0.0, to.z);
        if self.nodes.is_empty() {
            return vec![to_flat];
        }
        let start = self.nearest_node(from);
        let goal = self.nearest_node(to_flat);
        let node_ids = self.route_nodes(start, goal);
        let mut pts: Vec<Vec3> = Vec::with_capacity(node_ids.len() + 1);
        for idx in node_ids {
            let p = self.nodes[idx].pos;
            pts.push(Vec3::new(p.x, 0.0, p.y));
        }
        if pts
            .last()
            .is_none_or(|last| last.distance(to_flat) > 0.05)
        {
            pts.push(to_flat);
        }
        pts
    }

    pub fn desk_spot(&self, desk_index: usize) -> Option<NavSpot> {
        self.desk_spots.get(desk_index).copied()
    }

    pub fn desk_count(&self) -> usize {
        self.desk_spots.len()
    }

    pub fn station_spots(&self, kind: StationKind) -> &[NavSpot] {
        self.stations.get(&kind).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn station_spot_for(&self, kind: StationKind, seed: usize) -> NavSpot {
        let list = self.station_spots(kind);
        if !list.is_empty() {
            return list[seed % list.len()];
        }
        self.desk_spots
            .first()
            .copied()
            .unwrap_or(self.door_in)
    }

    /// Picks a wall-safe briefing spot beside or behind a parent worker (`workers.js::nearBoss`).
    pub fn near_boss_spot(&self, boss_pos: Vec3, boss_yaw: f32, sub_idx: usize) -> NavSpot {
        let spec = LayoutSpec::for_layout(self.layout);
        let angles = [
            PI,
            FRAC_PI_2,
            -FRAC_PI_2,
            3.0 * FRAC_PI_4,
            -3.0 * FRAC_PI_4,
            FRAC_PI_4,
            -FRAC_PI_4,
        ];
        for r in [0.95_f32, 1.35, 1.75] {
            for k in 0..angles.len() {
                let a = boss_yaw + angles[(k + sub_idx) % angles.len()];
                let cand = Vec3::new(boss_pos.x + a.sin() * r, 0.0, boss_pos.z + a.cos() * r);
                if spec.inside(cand.x, cand.z, 0.45) && !self.crosses_wall(boss_pos, cand) {
                    let node_idx = self.nearest_node(cand);
                    return NavSpot {
                        pos: cand,
                        face: face_to(cand.x, cand.z, boss_pos.x, boss_pos.z),
                        node_idx,
                        sit: false,
                    };
                }
            }
        }
        self.station_spot_for(StationKind::Meeting, sub_idx)
    }

    pub fn door_in(&self) -> NavSpot {
        self.door_in
    }

    pub fn door_out(&self) -> NavSpot {
        self.door_out
    }

    pub fn add_wet_strip(&mut self, a: Vec2, b: Vec2) {
        self.wet_strips.push(WetStrip { a, b });
    }

    pub fn clear_wet_strips(&mut self) {
        self.wet_strips.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_path_respects_walls_and_uses_doorway(
        graph: &NavGraph,
        label: &str,
        from: Vec3,
        to: Vec3,
        expected_doorways: &[&str],
    ) {
        let path = graph.shortest_path(from, to);
        assert!(
            path.len() >= 2,
            "{label}: expected multi-waypoint doorway path, got {} points: {path:?}",
            path.len()
        );
        let mut prev = from;
        for (i, &wp) in path.iter().enumerate() {
            assert!(
                !graph.crosses_wall(prev, wp),
                "{label}: leg {i} ({prev:?} -> {wp:?}) crosses an interior partition wall!"
            );
            prev = wp;
        }
        let doorways = graph.doorway_nodes();
        assert!(!doorways.is_empty(), "{label}: doorway_nodes() must not be empty");
        for &door_name in expected_doorways {
            let door_pos = graph
                .node_pos(door_name)
                .unwrap_or_else(|| panic!("{label}: missing expected doorway node {door_name}"));
            assert!(
                path.iter().any(|p| p.distance(door_pos) < 0.05),
                "{label}: path did not pass through doorway node {door_name} ({door_pos:?}); path={path:?}"
            );
        }
    }

    #[test]
    fn pro_nav_graph_routes_between_rooms_strictly_through_doorways_without_crossing_walls() {
        let nav = NavGraph::for_layout(OfficeLayout::Pro);
        assert!(nav.node_count() >= 55, "Pro graph should have all Oracle NODES");
        assert!(!nav.walls().is_empty(), "Pro graph must encode solid partition segments");

        // Every graph edge itself must be 100% wall-free.
        for (u, neighbors) in nav.adj.iter().enumerate() {
            let pu = Vec3::new(nav.nodes[u].pos.x, 0.0, nav.nodes[u].pos.y);
            for &v in neighbors {
                let pv = Vec3::new(nav.nodes[v].pos.x, 0.0, nav.nodes[v].pos.y);
                assert!(
                    !nav.crosses_wall(pu, pv),
                    "Pro edge {} ({pu:?}) -> {} ({pv:?}) crosses a partition wall!",
                    nav.nodes[u].name,
                    nav.nodes[v].name
                );
            }
        }

        // Director office -> Dev desk 0: must pass through L0 <-> DRi doorway at x=-3.1, z=-3.0
        let director_desk = Vec3::new(-5.4, 0.0, -6.75);
        let dev_desk_0 = nav.desk_spot(0).expect("desk 0 spot").pos;
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Director -> Desk 0",
            director_desk,
            dev_desk_0,
            &["DRi", "L0"],
        );

        // Lobby (Lift) -> Dev desk 4: must pass through Lobby double doors LB <-> L2 at x=-3.6, z=3.3
        let lift_in = nav.door_in().pos;
        let dev_desk_4 = nav.desk_spot(4).expect("desk 4 spot").pos;
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Lobby -> Desk 4",
            lift_in,
            dev_desk_4,
            &["LB", "L2"],
        );

        // Meeting room -> Dev desk 2: must pass through MTd <-> MTi doorway at x=2.35, z=-3.0
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Meeting -> Desk 2",
            Vec3::new(0.2, 0.0, -5.6),
            nav.desk_spot(2).expect("desk 2 spot").pos,
            &["MTi", "MTd"],
        );

        // Library -> Dev desk 6: must pass through LBd <-> LBi doorway at x=3.8, z=-3.0
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Library -> Desk 6",
            Vec3::new(6.9, 0.0, -4.1),
            nav.desk_spot(6).expect("desk 6 spot").pos,
            &["LBi", "LBd"],
        );

        // Server room -> Dev desk 0: must pass through SVi <-> SVo doorway at x=-3.6, z=-1.55
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Server -> Desk 0",
            Vec3::new(-6.55, 0.0, -1.47),
            dev_desk_0,
            &["SVi", "SVo"],
        );

        // Pantry -> Lounge: must pass through PA5 <-> LG1 doorway at x=9.3, z=0.6
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Pantry -> Lounge",
            Vec3::new(12.8, 0.0, -4.9),
            Vec3::new(11.0, 0.0, 3.8),
            &["PA5", "LG1"],
        );

        // Restroom -> Dev desk: must pass through WCi <-> WCo -> LB -> L2
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "Restroom -> Desk 0",
            Vec3::new(-7.25, 0.0, 8.5),
            dev_desk_0,
            &["WCi", "WCo", "LB", "L2"],
        );
    }

    #[test]
    fn hq_nav_graph_routes_from_department_wing_to_pro_core_without_crossing_walls() {
        let nav = NavGraph::for_layout(OfficeLayout::Hq);
        assert_eq!(nav.desk_spots.len(), 44);

        for (u, neighbors) in nav.adj.iter().enumerate() {
            let pu = Vec3::new(nav.nodes[u].pos.x, 0.0, nav.nodes[u].pos.y);
            for &v in neighbors {
                let pv = Vec3::new(nav.nodes[v].pos.x, 0.0, nav.nodes[v].pos.y);
                assert!(
                    !nav.crosses_wall(pu, pv),
                    "HQ edge {} ({pu:?}) -> {} ({pv:?}) crosses a partition wall!",
                    nav.nodes[u].name,
                    nav.nodes[v].name
                );
            }
        }

        // Dept 3 (front-right wing, desk 40) -> Director office: must pass D3i -> HB1 -> core -> L0 -> DRi
        let wing_desk = nav.desk_spot(40).expect("HQ desk 40").pos;
        let director_desk = Vec3::new(-5.4, 0.0, -6.75);
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "HQ Dept 3 -> Director",
            wing_desk,
            director_desk,
            &["D3i", "HB1", "L0", "DRi"],
        );

        // Dept 0 (back-left wing, desk 14) -> Dev core desk 0: must pass D0i -> HAL -> L4
        let dept0_desk = nav.desk_spot(14).expect("HQ desk 14").pos;
        let core_desk = nav.desk_spot(0).expect("core desk 0").pos;
        assert_path_respects_walls_and_uses_doorway(
            &nav,
            "HQ Dept 0 -> Core Desk 0",
            dept0_desk,
            core_desk,
            &["D0i", "HAL", "L4"],
        );
    }

    #[test]
    fn cozy_nav_graph_connects_all_desks_and_stations_and_avoids_wet_strips() {
        let mut nav = NavGraph::for_layout(OfficeLayout::Cozy);
        assert_eq!(nav.desk_spots.len(), 9);
        assert_eq!(nav.node_count(), 24);

        let a = nav.node_pos("A").unwrap();
        let e = nav.node_pos("E").unwrap();
        let dry_path = nav.shortest_path(a, e);
        assert!(dry_path.len() >= 4);

        // Place a wet strip across the top aisle (T0..T2); Dijkstra should detour via U row
        nav.add_wet_strip(Vec2::new(-2.5, -3.15), Vec2::new(2.5, -3.15));
        let detour_path = nav.shortest_path(a, e);
        let t1 = nav.node_pos("T1").unwrap();
        assert!(
            !detour_path.iter().any(|p| p.distance(t1) < 0.1),
            "wet strip on T aisle should force detour off T1; got {detour_path:?}"
        );
    }
}
