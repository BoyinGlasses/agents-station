use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapInfo {
    pub off: &'static str,
    pub work: &'static str,
    pub why: &'static str,
    pub by: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakInfo {
    pub days: u32,
    pub makeup: String,
}

pub const SWAPS: [(&str, &str, &str, &str); 2] = [
    (
        "2026-01-02",
        "2026-01-10",
        "newYear",
        "Văn phòng Chính phủ 12729/VPCP-KGVX, 25/12/2025; Bộ Nội vụ",
    ),
    (
        "2026-08-31",
        "2026-08-22",
        "national",
        "Bộ Nội vụ 9441/TB-BNV, 16/10/2025",
    ),
];

fn tet_day(y: i32) -> Option<(u32, u32)> {
    match y {
        2026 => Some((2, 17)),
        2027 => Some((2, 6)),
        2028 => Some((1, 26)),
        2029 => Some((2, 13)),
        2030 => Some((2, 3)),
        2031 => Some((1, 23)),
        2032 => Some((2, 11)),
        2033 => Some((1, 31)),
        _ => None,
    }
}

fn hung_day(y: i32) -> Option<(u32, u32)> {
    match y {
        2026 => Some((4, 26)),
        2027 => Some((4, 16)),
        2028 => Some((4, 4)),
        2029 => Some((4, 23)),
        2030 => Some((4, 12)),
        2031 => Some((4, 1)),
        2032 => Some((4, 19)),
        2033 => Some((4, 9)),
        _ => None,
    }
}

fn announced_tet(y: i32) -> Option<&'static [(u32, u32)]> {
    match y {
        2026 => Some(&[(2, 16), (2, 17), (2, 18), (2, 19), (2, 20)]),
        2027 => Some(&[(2, 4), (2, 5), (2, 6), (2, 7), (2, 8)]),
        _ => None,
    }
}

fn announced_national(y: i32) -> &'static [(u32, u32)] {
    match y {
        2026 => &[(9, 1), (9, 2)],
        2027 => &[(9, 2), (9, 3)],
        _ => &[(9, 1), (9, 2)],
    }
}

pub fn day_key(d: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

fn is_weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

fn add_days(d: NaiveDate, n: i64) -> NaiveDate {
    if n >= 0 {
        d.checked_add_days(Days::new(n as u64)).unwrap()
    } else {
        d.checked_sub_days(Days::new((-n) as u64)).unwrap()
    }
}

pub fn swap_of(d: NaiveDate) -> Option<SwapInfo> {
    let k = day_key(d);
    for (off, work, why, by) in SWAPS {
        if off == k || work == k {
            return Some(SwapInfo { off, work, why, by });
        }
    }
    None
}

fn holidays(y: i32) -> BTreeMap<NaiveDate, &'static str> {
    let mut map: BTreeMap<NaiveDate, &'static str> = BTreeMap::new();
    let mut add = |d: NaiveDate, what: &'static str| {
        map.entry(d).or_insert(what);
    };

    add(NaiveDate::from_ymd_opt(y, 1, 1).unwrap(), "newYear");
    if let Some(days) = announced_tet(y) {
        for &(m, d) in days {
            add(NaiveDate::from_ymd_opt(y, m, d).unwrap(), "tet");
        }
    } else if let Some((m, d)) = tet_day(y) {
        let base = NaiveDate::from_ymd_opt(y, m, d).unwrap();
        for k in -1..4 {
            add(add_days(base, k), "tet");
        }
    }
    if let Some((m, d)) = hung_day(y) {
        add(NaiveDate::from_ymd_opt(y, m, d).unwrap(), "hung");
    }
    add(NaiveDate::from_ymd_opt(y, 4, 30).unwrap(), "reunif");
    add(NaiveDate::from_ymd_opt(y, 5, 1).unwrap(), "labour");
    for &(m, d) in announced_national(y) {
        add(NaiveDate::from_ymd_opt(y, m, d).unwrap(), "national");
    }
    if y >= 2026 {
        add(NaiveDate::from_ymd_opt(y, 11, 24).unwrap(), "culture");
    }

    let keys: Vec<NaiveDate> = map.keys().copied().collect();
    for d in keys {
        if !is_weekend(d) {
            continue;
        }
        let mut n = add_days(d, 1);
        while is_weekend(n) || map.contains_key(&n) {
            n = add_days(n, 1);
        }
        map.insert(n, "comp");
    }
    map
}

pub fn holiday_of(d: NaiveDate) -> &'static str {
    holidays(d.year()).get(&d).copied().unwrap_or("")
}

pub fn day_kind(d: NaiveDate) -> &'static str {
    let k = day_key(d);
    if SWAPS.iter().any(|(off, _, _, _)| *off == k) {
        return "swap";
    }
    if SWAPS.iter().any(|(_, work, _, _)| *work == k) {
        return "makeup";
    }
    if !holiday_of(d).is_empty() {
        return "holiday";
    }
    if is_weekend(d) { "weekend" } else { "work" }
}

pub fn is_workday(d: NaiveDate) -> bool {
    matches!(day_kind(d), "work" | "makeup")
}

pub fn break_after(d: NaiveDate) -> BreakInfo {
    let mut x = add_days(d, 1);
    let mut days = 0u32;
    let mut makeup = String::new();
    while days < 31 && !is_workday(x) {
        if makeup.is_empty() {
            let k = day_key(x);
            if let Some((_, work, _, _)) = SWAPS.iter().find(|(off, _, _, _)| *off == k) {
                makeup = (*work).to_owned();
            }
        }
        days += 1;
        x = add_days(x, 1);
    }
    BreakInfo { days, makeup }
}

pub fn quarter_start(d: NaiveDate) -> NaiveDate {
    let q_month = ((d.month() - 1) / 3) * 3 + 1;
    let mut x = NaiveDate::from_ymd_opt(d.year(), q_month, 1).unwrap();
    for _ in 0..31 {
        if is_workday(x) {
            break;
        }
        x = add_days(x, 1);
    }
    x
}

pub fn prev_workday(d: NaiveDate) -> NaiveDate {
    let mut x = add_days(d, -1);
    for _ in 0..31 {
        if is_workday(x) {
            break;
        }
        x = add_days(x, -1);
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(iso: &str) -> NaiveDate {
        NaiveDate::parse_from_str(iso, "%Y-%m-%d").unwrap()
    }

    fn kinds(from: &str, to: &str) -> Vec<(String, &'static str)> {
        let mut cur = d(from);
        let end = d(to);
        let mut out = Vec::new();
        while cur <= end {
            out.push((day_key(cur), day_kind(cur)));
            cur = add_days(cur, 1);
        }
        out
    }

    #[test]
    fn weekdays_holidays_swaps_and_breaks_match_oracle() {
        let week: Vec<&str> = kinds("2026-11-02", "2026-11-08")
            .into_iter()
            .map(|(_, k)| k)
            .collect();
        assert_eq!(week, vec!["work", "work", "work", "work", "work", "weekend", "weekend"]);

        let tet = kinds("2026-02-13", "2026-02-23");
        assert_eq!(tet[0].1, "work");
        assert_eq!(tet[10].1, "work");
        let mid: Vec<&str> = tet[1..10].iter().map(|(_, k)| *k).collect();
        assert_eq!(
            mid,
            vec!["weekend", "weekend", "holiday", "holiday", "holiday", "holiday", "holiday", "weekend", "weekend"]
        );
        assert_eq!(holiday_of(d("2026-04-26")), "hung");
        assert_eq!(holiday_of(d("2026-04-27")), "comp");
        assert_eq!(holiday_of(d("2026-11-24")), "culture");
        assert!(is_workday(d("2026-11-25")));

        for iso in ["2027-02-04", "2027-02-05", "2027-02-08", "2027-02-09", "2027-02-10"] {
            assert_eq!(day_kind(d(iso)), "holiday", "{iso}");
        }
        assert_eq!(holiday_of(d("2027-02-09")), "comp");
        assert_eq!(holiday_of(d("2027-05-03")), "comp");
        assert_eq!(holiday_of(d("2027-09-03")), "national");
        assert!(is_workday(d("2027-09-01")));

        assert_eq!(holiday_of(d("2028-01-31")), "comp");
        assert_eq!(day_key(prev_workday(d("2026-11-02"))), "2026-10-30");
        assert_eq!(day_key(prev_workday(d("2027-02-11"))), "2027-02-03");
        assert_eq!(day_key(quarter_start(d("2027-02-15"))), "2027-01-04");
        assert_eq!(day_key(quarter_start(d("2028-01-10"))), "2028-01-04");

        assert_eq!(day_kind(d("2026-01-02")), "swap");
        assert_eq!(day_kind(d("2026-01-10")), "makeup");
        assert_eq!(swap_of(d("2026-01-10")), swap_of(d("2026-01-02")));
        assert_eq!(break_after(d("2025-12-31")), BreakInfo { days: 4, makeup: "2026-01-10".into() });
        assert_eq!(break_after(d("2026-08-28")), BreakInfo { days: 5, makeup: "2026-08-22".into() });
        assert_eq!(break_after(d("2026-02-13")), BreakInfo { days: 9, makeup: String::new() });
        assert_eq!(day_key(prev_workday(d("2026-01-12"))), "2026-01-10");
    }
}
