use crate::chibi::{ChibiConfig, ChibiLook, ChibiWorker, WorkerAnim, spawn_chibi};
use crate::layout::LayoutSpec;
use crate::palette::WorldPalette;
use bevy::prelude::*;
use std::f32::consts::PI;

pub const KARAOKE_SONGS: [(&str, &str); 5] = [
    ("Lạc Trôi", "Người theo hương hoa mây mù giăng lối ♪"),
    ("Sóng Gió", "Hồng trần trên đôi cánh tay ♫"),
    ("Nàng Thơ", "Em, ngày em đánh rơi nụ cười vào anh ♪"),
    ("Có Chàng Trai Viết Lên Cây", "Có chàng trai viết lên cây lời yêu thương ♫"),
    ("Bật Tình Yêu Lên", "Rót mật ngọt vào tai, bật tình yêu lên ♪"),
];

pub const TEAM_CHAT_LINES: [&str; 6] = [
    "Build xanh rồi, đi làm ly cà phê sữa đá thôi!",
    "Test pass 100%, chuẩn bị ship bản release!",
    "Trưa nay cơm tấm hay bún bò Huế đây cả nhà?",
    "Ai quên tắt dev server port 3000 kìa!",
    "RAM chạy còn dưới 200 MB luôn, mượt quá!",
    "Chiều nay 17h30 ra sân pickleball nhé!",
];

/// Spawns the active agent cast seated at desks + office NPCs (director, HR, lounge gamers, karaoke singer).
pub fn spawn_office_cast(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
    look: ChibiLook,
) {
    let cast = [
        ("claude", "Claude", WorkerAnim::Work),
        ("codex", "Codex", WorkerAnim::Work),
        ("opencode", "OpenCode", WorkerAnim::Work),
        ("zcode", "ZCode", WorkerAnim::Wait),
        ("antigravity", "Antigravity", WorkerAnim::Work),
        ("claude", "Mai", WorkerAnim::Rest),
    ];

    for (i, (agent, name, anim)) in cast.iter().enumerate() {
        let Some(desk) = spec.desks.get(i) else {
            break;
        };
        let mut cfg = ChibiConfig::for_agent(agent, name, i);
        cfg.look = look;
        let rot = Quat::from_rotation_y(desk.ry);
        let seat_pos = Vec3::new(desk.x, 0.0, desk.z) + rot * Vec3::new(0.0, 0.0, -0.62);
        spawn_chibi(
            commands,
            palette,
            materials,
            &cfg,
            seat_pos,
            desk.ry,
            *anim,
            i as f32 * 0.9,
            true,
        );
    }

    // Pro / HQ office NPCs: Director in walnut office, HR/Accountant walking corridor, Lounge gamer, Karaoke singer
    if spec.layout.is_pro() {
        let mut dir_cfg = ChibiConfig::for_agent("npc", "Giám đốc", 10);
        dir_cfg.shirt = "#2C3E50";
        dir_cfg.accessory = Some("glasses");
        dir_cfg.look = look;
        spawn_chibi(commands, palette, materials, &dir_cfg, Vec3::new(-5.4, 0.0, -6.55), 0.0, WorkerAnim::Work, 0.5, true);

        let mut hr_cfg = ChibiConfig::for_agent("npc", "HR Lan", 11);
        hr_cfg.shirt = "#D96B90";
        hr_cfg.look = look;
        let hr_ent = spawn_chibi(commands, palette, materials, &hr_cfg, Vec3::new(-1.5, 0.0, -2.3), PI * 0.5, WorkerAnim::Walk, 1.4, false);
        commands.entity(hr_ent).insert(PatrolPath {
            a: Vec3::new(-2.8, 0.0, -2.3),
            b: Vec3::new(7.5, 0.0, -2.3),
            speed: 0.75,
        });

        // Lounge gamer on sofa
        let mut gamer_cfg = ChibiConfig::for_agent("npc", "Huy", 12);
        gamer_cfg.shirt = "#3F7CC4";
        gamer_cfg.accessory = Some("headphones");
        gamer_cfg.look = look;
        spawn_chibi(commands, palette, materials, &gamer_cfg, Vec3::new(10.7, 0.0, 3.85), PI, WorkerAnim::Rest, 2.2, true);

        // Karaoke singer inside booth
        if let Some([kx, kz, kry]) = spec.ktv {
            let mut sing_cfg = ChibiConfig::for_agent("npc", "An", 13);
            sing_cfg.shirt = "#FF5F9E";
            sing_cfg.look = look;
            spawn_chibi(commands, palette, materials, &sing_cfg, Vec3::new(kx, 0.08, kz), kry + PI, WorkerAnim::Sing, 3.1, false);
        }
    }
}

#[derive(Component)]
pub struct PatrolPath {
    pub a: Vec3,
    pub b: Vec3,
    pub speed: f32,
}

/// Smoothly animates seated workers (typing/waiting/sleeping bob) and patrolling NPCs.
pub fn animate_chibis_system(
    time: Res<Time>,
    mut q: Query<(&ChibiWorker, Option<&PatrolPath>, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (w, patrol, mut tf) in &mut q {
        if let Some(p) = patrol {
            let u = ((t * p.speed + w.phase).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let next = p.a.lerp(p.b, u);
            let dir = if (t * p.speed + w.phase).cos() >= 0.0 {
                p.b - p.a
            } else {
                p.a - p.b
            };
            tf.translation = next + Vec3::Y * ((t * 7.0 + w.phase).sin().abs() * 0.04);
            tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
            continue;
        }
        match w.anim {
            WorkerAnim::Work => {
                tf.translation.y = w.base_pos.y + (t * 6.0 + w.phase).sin().abs() * 0.018;
            }
            WorkerAnim::Wait => {
                tf.translation.y = w.base_pos.y + (t * 4.5 + w.phase).sin().abs() * 0.055;
            }
            WorkerAnim::Sing => {
                tf.translation.y = w.base_pos.y + (t * 5.0 + w.phase).sin().abs() * 0.04;
                tf.rotation = Quat::from_rotation_y(w.base_yaw + (t * 2.5 + w.phase).sin() * 0.25);
            }
            WorkerAnim::Sleep => {
                tf.translation.y = w.base_pos.y + (t * 1.4 + w.phase).sin() * 0.01;
            }
            WorkerAnim::Rest | WorkerAnim::Walk => {
                tf.translation.y = w.base_pos.y + (t * 2.2 + w.phase).sin() * 0.012;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chibi_configs_cover_all_five_agents_and_karaoke_songs() {
        for (i, ag) in ["claude", "codex", "opencode", "zcode", "antigravity"].iter().enumerate() {
            let c = ChibiConfig::for_agent(ag, ag, i);
            assert_eq!(c.agent, *ag);
            assert!(c.shirt.starts_with('#'));
        }
        assert_eq!(KARAOKE_SONGS.len(), 5);
        assert_eq!(TEAM_CHAT_LINES.len(), 6);
    }
}
