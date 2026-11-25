use crate::shared::workdays::day_kind;
use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

fn fnv1a_utf16(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for ch in s.chars() {
        let mut buf = [0u16; 2];
        let units = ch.encode_utf16(&mut buf);
        h ^= units[0] as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

fn unpadded_day_key(d: NaiveDate) -> String {
    format!("{}-{}-{}", d.year(), d.month(), d.day())
}

pub fn home_share(d: NaiveDate) -> u32 {
    let k = day_kind(d);
    if k != "work" && k != "makeup" {
        return 0;
    }
    if d.weekday() == Weekday::Mon { 55 } else { 30 }
}

pub fn brings_lunch(id: &str, d: NaiveDate, force: bool) -> bool {
    if id.is_empty() {
        return false;
    }
    force || (fnv1a_utf16(&format!("{id}:{}:comnha", unpadded_day_key(d))) % 100) < home_share(d)
}

pub fn warm_at(id: &str, d: NaiveDate) -> u32 {
    705 + (fnv1a_utf16(&format!("{id}:{}:warm", unpadded_day_key(d))) % 30)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LunchBox {
    pub kind: &'static str,
    pub smelly: bool,
    pub pop: bool,
    pub pantry: bool,
}

const BOX_KINDS: [&str; 5] = ["blue", "green", "pink", "glass", "inox"];

pub fn lunch_of(id: &str, d: NaiveDate) -> LunchBox {
    let h = fnv1a_utf16(&format!("{id}:{}:box", unpadded_day_key(d)));
    LunchBox {
        kind: BOX_KINDS[(fnv1a_utf16(id) % 5) as usize],
        smelly: (h % 100) < 22,
        pop: ((h >> 8) % 100) < 14,
        pantry: ((h >> 16) % 3) != 0,
    }
}

pub fn heat_secs(id: &str, d: NaiveDate, quick: bool) -> u32 {
    let k = fnv1a_utf16(&format!("{id}:{}:secs", unpadded_day_key(d))) % 61;
    if quick { 15 + (k % 11) } else { 120 + k }
}

pub fn warm_window(min: u32) -> bool {
    (705..780).contains(&min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn comnha_distribution_and_determinism_match_oracle() {
        let mon = d(2026, 11, 2);
        let tue = d(2026, 11, 3);
        let wed = d(2026, 11, 4);
        let sat = d(2026, 11, 7);
        let sun = d(2026, 11, 8);
        let ids: Vec<String> = (0..2000).map(|i| format!("sess-{i}")).collect();
        let share = |dt| ids.iter().filter(|id| brings_lunch(id, dt, false)).count() as f64 / ids.len() as f64;

        assert_eq!(home_share(mon), 55);
        assert_eq!(home_share(tue), 30);
        assert_eq!(home_share(sat), 0);
        assert_eq!(home_share(sun), 0);
        assert!((share(mon) - 0.55).abs() < 0.04);
        assert!((share(tue) - 0.30).abs() < 0.04);
        assert!(share(mon) > share(tue));
        assert_eq!(share(sat), 0.0);
        assert!(!brings_lunch("", tue, true));
        assert!(brings_lunch("anyone", sat, true));
        assert!(ids.iter().any(|id| brings_lunch(id, tue, false) != brings_lunch(id, wed, false)));

        let mins: Vec<u32> = ids.iter().map(|id| warm_at(id, tue)).collect();
        assert_eq!(*mins.iter().min().unwrap(), 705);
        assert_eq!(*mins.iter().max().unwrap(), 734);
        assert!(mins.iter().all(|&m| warm_window(m)));
        assert!(!warm_window(704) && !warm_window(780) && warm_window(779));

        for id in &ids[..300] {
            let s = heat_secs(id, tue, false);
            let q = heat_secs(id, tue, true);
            assert!((120..=180).contains(&s));
            assert!((15..=25).contains(&q));
        }
        assert_eq!(lunch_of("x", tue).kind, lunch_of("x", sat).kind);
    }
}
