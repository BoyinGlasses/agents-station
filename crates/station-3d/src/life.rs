use crate::layout::LayoutSpec;
use crate::palette::{WorldPalette, spawn_box, spawn_cyl, spawn_quad, spawn_sphere};
use bevy::prelude::*;
use station_canvas2d::boards_and_social::{PhotostripInput, StripPerson, draw_photostrip};
use station_canvas2d::{FontEngine, hex};
use std::f32::consts::PI;

pub const VN_WEATHER_CITIES: [(&str, &str, f64, f64); 8] = [
    ("hn", "Hà Nội", 21.03, 105.85),
    ("hcm", "TP. Hồ Chí Minh", 10.78, 106.70),
    ("dn", "Đà Nẵng", 16.05, 108.20),
    ("hue", "Huế", 16.46, 107.59),
    ("hp", "Hải Phòng", 20.86, 106.68),
    ("ct", "Cần Thơ", 10.03, 105.78),
    ("dl", "Đà Lạt", 11.94, 108.44),
    ("nt", "Nha Trang", 12.24, 109.19),
];

/// Normalizes a weather city code or display name into `(canonical_code, display_name)`
/// so startup prefs, `SetWeatherCity`, and Settings UI highlights stay 100% consistent.
pub fn weather_city_display(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("hcmc") || trimmed.eq_ignore_ascii_case("saigon") {
        return ("hcm".to_owned(), "TP. Hồ Chí Minh".to_owned());
    }
    if trimmed.eq_ignore_ascii_case("hanoi") {
        return ("hn".to_owned(), "Hà Nội".to_owned());
    }
    for (code, name, _, _) in VN_WEATHER_CITIES {
        if code.eq_ignore_ascii_case(trimmed) || name.eq_ignore_ascii_case(trimmed) {
            return (code.to_owned(), name.to_owned());
        }
    }
    let clean = if trimmed.is_empty() { "hcm" } else { trimmed };
    (clean.to_ascii_lowercase(), clean.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeasonTheme {
    #[default]
    Normal,
    Halloween,
    Tet,
    MidAutumn,
    Christmas,
}

impl SeasonTheme {
    /// Resolves the active office seasonal decoration theme from a calendar date (`env.js:67-71`):
    /// - `Halloween`: Oct 20 – Nov 2
    /// - `Christmas`: Dec 10 – Dec 31
    /// - `Tet`: Vietnamese Lunar New Year window (`[-14..=+7]` days around Mùng 1 Tết)
    /// - `Normal`: all other dates
    pub fn from_date(year: i32, month: u32, day: u32) -> Self {
        let md = month * 100 + day;
        if (1020..=1102).contains(&md) {
            return Self::Halloween;
        }
        if (1210..=1231).contains(&md) {
            return Self::Christmas;
        }
        if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
            // Exact Vietnamese Lunar New Year (Mùng 1 Tết, UTC+7, matching `workdays.js:19`)
            const TET_MUNG1: [(i32, u32, u32); 12] = [
                (2024, 2, 10),
                (2025, 1, 29),
                (2026, 2, 17),
                (2027, 2, 6),
                (2028, 1, 26),
                (2029, 2, 13),
                (2030, 2, 3),
                (2031, 1, 23),
                (2032, 2, 11),
                (2033, 1, 31),
                (2034, 2, 19),
                (2035, 2, 8),
            ];
            for &(ty, tm, td) in &TET_MUNG1 {
                if ty == year || ty == year + 1 {
                    if let Some(tet) = chrono::NaiveDate::from_ymd_opt(ty, tm, td) {
                        let diff = date.signed_duration_since(tet).num_days();
                        if (-14..=7).contains(&diff) {
                            return Self::Tet;
                        }
                    }
                }
            }
        }
        Self::Normal
    }

    /// Resolves today's seasonal theme using local wall clock.
    pub fn from_local_today() -> Self {
        use chrono::Datelike;
        let today = chrono::Local::now().date_naive();
        Self::from_date(today.year(), today.month(), today.day())
    }
}

#[derive(Resource, Debug, Clone)]
pub struct OfficeLifeState {
    pub weather_city: String,
    pub weather_code: Option<u32>,
    pub temp_c: Option<f32>,
    pub rain_intensity: f32,
    pub storm: bool,
    pub nap_active: bool,
    pub drill_active: bool,
    pub incident_active: bool,
    pub lift_in_service: bool,
    pub season: SeasonTheme,
    pub earned_trophies: usize,
}

impl Default for OfficeLifeState {
    fn default() -> Self {
        Self {
            weather_city: "hcm".to_owned(),
            weather_code: Some(1),
            temp_c: Some(31.0),
            rain_intensity: 0.0,
            storm: false,
            nap_active: false,
            drill_active: false,
            incident_active: false,
            lift_in_service: false,
            season: SeasonTheme::Normal,
            earned_trophies: 3,
        }
    }
}

impl OfficeLifeState {
    pub fn apply_open_meteo(&mut self, code: u32, temp_c: f32) {
        self.weather_code = Some(code);
        self.temp_c = Some(temp_c);
        self.storm = code >= 95;
        self.rain_intensity = if code >= 95 {
            1.0
        } else if (80..=82).contains(&code) {
            0.55 + (code - 80) as f32 * 0.2
        } else if (61..=67).contains(&code) {
            0.35 + (code - 61) as f32 * 0.12
        } else if (51..=57).contains(&code) {
            0.18
        } else {
            0.0
        };
    }

    pub fn weather_icon(&self) -> &'static str {
        match self.weather_code {
            Some(0) => "☀",
            Some(1..=2) => "⛅",
            Some(3) => "☁",
            Some(4..=48) => "🌫",
            Some(49..=67) => "🌧",
            Some(68..=77) => "❄",
            Some(78..=82) => "🌦",
            Some(_) => "⛈",
            None => "",
        }
    }
}

#[derive(Component)]
pub struct RainDrop {
    pub x: f32,
    pub z: f32,
    pub speed: f32,
}

/// Spawns lobby Ông Địa altar, trophy shelf, whiteboard photostrip, and seasonal décor.
pub fn spawn_life_elements(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    life: &OfficeLifeState,
) {
    // 1. Vietnamese Ông Địa / Thần Tài altar in lobby corner (Pro/HQ)
    if spec.layout.is_pro() {
        let wood = pal.mat(materials, "#6E2B1E", 0.45, 0.05);
        let gold = pal.mat(materials, "#E2B04A", 0.3, 0.65);
        let candle = pal.emissive(materials, "#FF5238", 1.8);
        let ax = -7.35;
        let az = 0.95;
        spawn_box(commands, pal, wood.clone(), Vec3::new(ax, 0.36, az), Vec3::new(0.54, 0.72, 0.48));
        spawn_box(commands, pal, gold, Vec3::new(ax, 0.76, az), Vec3::new(0.6, 0.08, 0.54));
        spawn_sphere(commands, pal, candle.clone(), Vec3::new(ax - 0.16, 0.44, az + 0.18), Vec3::splat(0.06));
        spawn_sphere(commands, pal, candle, Vec3::new(ax + 0.16, 0.44, az + 0.18), Vec3::splat(0.06));
    }

    // 2. Pinned Korean photostrip on lower right of whiteboard (photostrips.js)
    {
        let [bx, by, bz, bry] = spec.board;
        let rot = Quat::from_rotation_y(bry);
        let strip_surf = draw_photostrip(
            &PhotostripInput {
                tag: "v1.0.0".into(),
                day: "25/11".into(),
                seed: 2,
                people: vec![
                    StripPerson { hair: hex("#3B2618"), skin: hex("#F5D0B5"), shirt: hex("#D97757") },
                    StripPerson { hair: hex("#1E1E24"), skin: hex("#EBC3A0"), shirt: hex("#5F8FD8") },
                    StripPerson { hair: hex("#5A3A2A"), skin: hex("#F7DAC4"), shirt: hex("#3E9B74") },
                ],
            },
            fonts,
        );
        let strip_m = pal.textured_lit(images, materials, strip_surf, 0.55);
        spawn_quad(
            commands,
            pal,
            strip_m,
            Vec3::new(bx + 0.075, by - 0.55, bz - 0.85),
            Vec2::new(0.16, 0.44),
            rot * Quat::from_rotation_y(PI * 0.5),
        );
    }

    // 3. Trophy shelf figurines (trophies.js)
    {
        let [sx, sz, _] = spec.shelf;
        let gold = pal.mat(materials, "#F2C230", 0.28, 0.75);
        let silver = pal.mat(materials, "#CFD6DF", 0.25, 0.8);
        for i in 0..life.earned_trophies.min(5) {
            let tm = if i % 2 == 0 { gold.clone() } else { silver.clone() };
            spawn_cyl(commands, pal, tm, Vec3::new(sx - 0.7 + i as f32 * 0.34, 2.42, sz), 0.14, 0.22);
        }
    }

    // 4. Seasonal accent decor
    match life.season {
        SeasonTheme::Halloween => {}
        SeasonTheme::Tet => {
            let blossom = pal.emissive(materials, "#FF85B3", 0.85);
            spawn_sphere(commands, pal, blossom, Vec3::new(-4.4, 1.35, 3.3), Vec3::splat(0.65));
        }
        SeasonTheme::MidAutumn => {
            let lantern = pal.emissive(materials, "#FF5E3A", 1.8);
            spawn_sphere(commands, pal, lantern, Vec3::new(spec.center.x, 3.4, spec.center.y), Vec3::splat(0.36));
        }
        SeasonTheme::Christmas => {
            let pine = pal.mat(materials, "#245C36", 0.75, 0.0);
            spawn_cyl(commands, pal, pine, Vec3::new(-4.4, 1.0, 3.3), 0.75, 1.8);
        }
        SeasonTheme::Normal => {}
    }
}

pub fn update_rain_system(time: Res<Time>, mut q: Query<(&RainDrop, &mut Transform)>) {
    let dt = time.delta_secs();
    for (r, mut tf) in &mut q {
        tf.translation.y -= r.speed * dt;
        if tf.translation.y < -0.5 {
            tf.translation.y = 14.0;
            tf.translation.x = r.x;
            tf.translation.z = r.z;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_codes_map_rain_storm_and_cities_match_oracle() {
        assert_eq!(VN_WEATHER_CITIES.len(), 8);
        let mut life = OfficeLifeState::default();
        life.apply_open_meteo(0, 32.0);
        assert_eq!(life.rain_intensity, 0.0);
        assert!(!life.storm);
        assert_eq!(life.weather_icon(), "☀");

        life.apply_open_meteo(95, 26.0);
        assert_eq!(life.rain_intensity, 1.0);
        assert!(life.storm);
        assert_eq!(life.weather_icon(), "⛈");

        assert_eq!(
            weather_city_display("hn"),
            ("hn".to_owned(), "Hà Nội".to_owned())
        );
        assert_eq!(
            weather_city_display("Hà Nội"),
            ("hn".to_owned(), "Hà Nội".to_owned())
        );
        assert_eq!(
            weather_city_display("HCMC"),
            ("hcm".to_owned(), "TP. Hồ Chí Minh".to_owned())
        );
    }

    #[test]
    fn season_theme_from_date_matches_halloween_christmas_and_lunar_tet_windows() {
        assert_eq!(SeasonTheme::from_date(2026, 10, 31), SeasonTheme::Halloween);
        assert_eq!(SeasonTheme::from_date(2026, 10, 20), SeasonTheme::Halloween);
        assert_eq!(SeasonTheme::from_date(2026, 11, 2), SeasonTheme::Halloween);
        assert_eq!(SeasonTheme::from_date(2026, 11, 3), SeasonTheme::Normal);

        assert_eq!(SeasonTheme::from_date(2026, 12, 10), SeasonTheme::Christmas);
        assert_eq!(SeasonTheme::from_date(2026, 12, 24), SeasonTheme::Christmas);
        assert_eq!(SeasonTheme::from_date(2026, 12, 31), SeasonTheme::Christmas);
        assert_eq!(SeasonTheme::from_date(2026, 12, 9), SeasonTheme::Normal);

        // Authentic Vietnamese Lunar New Year dates across multiple years
        assert_eq!(SeasonTheme::from_date(2025, 1, 29), SeasonTheme::Tet);
        assert_eq!(SeasonTheme::from_date(2025, 1, 16), SeasonTheme::Tet); // within 14d before
        assert_eq!(SeasonTheme::from_date(2025, 2, 5), SeasonTheme::Tet); // within 7d after
        assert_eq!(SeasonTheme::from_date(2026, 2, 17), SeasonTheme::Tet);
        assert_eq!(SeasonTheme::from_date(2026, 2, 3), SeasonTheme::Tet);
        assert_eq!(SeasonTheme::from_date(2026, 2, 24), SeasonTheme::Tet);
        assert_eq!(SeasonTheme::from_date(2027, 2, 6), SeasonTheme::Tet);

        // Ordinary non-holiday dates
        assert_eq!(SeasonTheme::from_date(2026, 5, 15), SeasonTheme::Normal);
        assert_eq!(SeasonTheme::from_date(2026, 8, 12), SeasonTheme::Normal);
    }
}
