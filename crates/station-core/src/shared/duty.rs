use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

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

pub const CHORES: [&str; 4] = ["cups", "trash", "plants", "table"];
pub const OWE_FROM: i32 = 480;
pub const PILE_MAX: usize = 24;

pub fn window_of(chore: &str) -> Option<(i32, i32)> {
    match chore {
        "plants" => Some((510, 1050)),
        "cups" => Some((810, 1080)),
        "trash" | "table" => Some((1015, 1100)),
        _ => None,
    }
}

pub fn due(chore: &str, min: i32) -> bool {
    window_of(chore).is_some_and(|(s, e)| min >= s && min < e)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwedChore {
    pub chore: String,
    pub who: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DutyPerson {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DutyState {
    pub v: u32,
    pub order: Vec<String>,
    pub next: usize,
    pub day: String,
    pub who: BTreeMap<String, String>,
    pub names: BTreeMap<String, String>,
    pub done: BTreeMap<String, i32>,
    pub owe: Vec<OwedChore>,
    pub seen: Vec<String>,
    pub left: usize,
    pub extra: usize,
    pub washed: i32,
    pub told: String,
    pub grumbled: String,
}

pub fn fresh() -> DutyState {
    DutyState {
        v: 1,
        order: Vec::new(),
        next: 0,
        day: String::new(),
        who: BTreeMap::new(),
        names: BTreeMap::new(),
        done: BTreeMap::new(),
        owe: Vec::new(),
        seen: Vec::new(),
        left: 0,
        extra: 0,
        washed: -1,
        told: String::new(),
        grumbled: String::new(),
    }
}

pub fn cup_moments(key: &str, day: &str) -> Vec<i32> {
    let h = fnv1a_utf16(&format!("{key}:{day}:cups"));
    let mut out = vec![480 + (h % 120) as i32, 750 + ((h >> 7) % 60) as i32];
    if ((h >> 14) % 10) < 7 {
        out.push(870 + ((h >> 18) % 90) as i32);
    }
    out
}

pub fn cups_between(keys: &[String], day: &str, from: i32, to: i32) -> usize {
    let mut n = 0;
    for k in keys {
        for m in cup_moments(k, day) {
            if m > from && m <= to {
                n += 1;
            }
        }
    }
    n
}

pub fn pile(st: &DutyState, min: i32) -> usize {
    (st.left + st.extra + cups_between(&st.seen, &st.day, st.washed, min)).min(PILE_MAX)
}

pub fn roll(st: &DutyState, today: &str, prev: &str) -> DutyState {
    if st.day == today {
        return st.clone();
    }
    let last = st.day == prev;
    let owe = if last {
        CHORES
            .iter()
            .filter_map(|&c| {
                let who = st.who.get(c)?;
                if st.done.contains_key(c) {
                    return None;
                }
                Some(OwedChore {
                    chore: c.to_owned(),
                    who: who.clone(),
                    name: st.names.get(who).cloned().unwrap_or_default(),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let left = if last { pile(st, 1440) } else { 0 };
    DutyState {
        day: today.to_owned(),
        who: BTreeMap::new(),
        done: BTreeMap::new(),
        owe,
        seen: Vec::new(),
        left,
        extra: 0,
        washed: -1,
        told: String::new(),
        grumbled: String::new(),
        ..st.clone()
    }
}

pub fn assign(st: &DutyState, here: &[DutyPerson]) -> DutyState {
    if !st.who.is_empty() || here.is_empty() {
        return st.clone();
    }
    let mut keys = Vec::new();
    let mut key_set = HashSet::new();
    let mut names = st.names.clone();
    for p in here {
        if key_set.insert(p.key.clone()) {
            keys.push(p.key.clone());
        }
        names.insert(p.key.clone(), p.name.clone());
    }
    let mut order = st.order.clone();
    let mut newcomers: Vec<String> = keys
        .iter()
        .filter(|k| !order.contains(*k))
        .cloned()
        .collect();
    newcomers.sort();
    order.extend(newcomers);
    if order.len() > 40 {
        let mut present: Vec<String> = order.iter().filter(|k| key_set.contains(*k)).cloned().collect();
        let absent: Vec<String> = order.iter().filter(|k| !key_set.contains(*k)).cloned().collect();
        present.extend(absent);
        present.truncate(40);
        order = present;
    }

    let n = CHORES.len().min(keys.len());
    let mut picked = Vec::with_capacity(n);
    let mut i = st.next % order.len();
    let mut last = i;
    for _ in 0..order.len() {
        if picked.len() >= n {
            break;
        }
        if key_set.contains(&order[i]) {
            picked.push(order[i].clone());
            last = i;
        }
        i = (i + 1) % order.len();
    }
    let mut who = BTreeMap::new();
    for (k, &c) in CHORES.iter().enumerate() {
        who.insert(c.to_owned(), picked[k % picked.len()].clone());
    }
    let order_set: HashSet<&String> = order.iter().collect();
    names.retain(|k, _| order_set.contains(k));

    let mut seen = st.seen.clone();
    for k in &keys {
        if !seen.contains(k) {
            seen.push(k.clone());
        }
    }
    seen.truncate(30);

    DutyState {
        order,
        next: (last + 1) % st.order.len().max(1).max(key_set.len()),
        who,
        names,
        seen,
        ..st.clone()
    }
}

pub fn assign_exact(st: &DutyState, here: &[DutyPerson]) -> DutyState {
    let mut out = assign(st, here);
    if !out.order.is_empty() && st.who.is_empty() && !here.is_empty() {
        // Recompute next precisely modulo out.order.len()
        let key_set: HashSet<&str> = here.iter().map(|p| p.key.as_str()).collect();
        let n = CHORES.len().min(key_set.len());
        let mut picked = 0;
        let mut i = st.next % out.order.len();
        let mut last = i;
        for _ in 0..out.order.len() {
            if picked >= n {
                break;
            }
            if key_set.contains(out.order[i].as_str()) {
                picked += 1;
                last = i;
            }
            i = (i + 1) % out.order.len();
        }
        out.next = (last + 1) % out.order.len();
    }
    out
}

pub fn saw(st: &DutyState, keys: &[String]) -> DutyState {
    if keys.iter().all(|k| st.seen.contains(k)) {
        return st.clone();
    }
    let mut seen = st.seen.clone();
    for k in keys {
        if !seen.contains(k) {
            seen.push(k.clone());
        }
    }
    seen.truncate(30);
    DutyState { seen, ..st.clone() }
}

pub fn done(st: &DutyState, chore: &str, min: i32, owed: bool) -> DutyState {
    if owed {
        let owe = st.owe.iter().filter(|o| o.chore != chore).cloned().collect();
        return DutyState { owe, ..st.clone() };
    }
    let mut d = st.done.clone();
    d.insert(chore.to_owned(), min);
    DutyState { done: d, ..st.clone() }
}

pub fn washed(st: &DutyState, min: i32, left: usize) -> DutyState {
    DutyState {
        left,
        extra: 0,
        washed: min,
        ..st.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people(keys: &[&str]) -> Vec<DutyPerson> {
        keys.iter()
            .map(|&k| DutyPerson {
                key: k.to_owned(),
                name: k.to_uppercase(),
            })
            .collect()
    }

    #[test]
    fn roster_rotation_owed_chores_and_cups_match_oracle() {
        let a = assign_exact(
            &roll(&fresh(), "2026-11-02", "2026-10-30"),
            &people(&["web", "api", "docs", "ops", "mobile"]),
        );
        assert_eq!(a.who.len(), 4);
        assert_eq!(a.who.values().collect::<HashSet<_>>().len(), 4);

        let b = assign_exact(
            &roll(&fresh(), "2026-11-02", "2026-10-30"),
            &people(&["web", "api"]),
        );
        let b_vals: Vec<&str> = CHORES.iter().map(|c| b.who[*c].as_str()).collect();
        assert_eq!(b_vals, vec!["api", "web", "api", "web"]);

        let mut st = assign_exact(
            &roll(&fresh(), "2026-11-02", "2026-10-30"),
            &people(&["a", "b", "c", "d", "e", "f"]),
        );
        let mon = st.who.clone();
        st = assign_exact(&st, &people(&["a", "b", "c", "d", "e", "f", "g"]));
        assert_eq!(st.who, mon);
        for c in CHORES {
            st = done(&st, c, 1000, false);
        }
        st = assign_exact(
            &roll(&st, "2026-11-03", "2026-11-02"),
            &people(&["a", "b", "c", "d", "e", "f"]),
        );
        let tue_vals: Vec<&str> = CHORES.iter().map(|c| st.who[*c].as_str()).collect();
        assert_eq!(tue_vals, vec!["e", "f", "a", "b"]);

        st = assign_exact(
            &roll(&st, "2026-11-04", "2026-11-03"),
            &people(&["a", "b", "d", "e", "f"]),
        );
        assert!(!st.who.values().any(|v| v == "c"));

        // Owed chores
        let mut st2 = assign_exact(
            &roll(&fresh(), "2026-11-02", "2026-10-30"),
            &people(&["a", "b", "c", "d"]),
        );
        st2 = done(&done(&st2, "plants", 560, false), "table", 1030, false);
        let tue2 = roll(&st2, "2026-11-03", "2026-11-02");
        let owe_chores: Vec<&str> = tue2.owe.iter().map(|o| o.chore.as_str()).collect();
        assert_eq!(owe_chores, vec!["cups", "trash"]);
        assert_eq!(done(&tue2, "cups", 490, true).owe.len(), 1);
        assert!(roll(&st2, "2026-11-05", "2026-11-04").owe.is_empty());

        // Cups pile
        let s_keys: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|s| (*s).to_owned()).collect();
        let st3 = saw(
            &assign_exact(&roll(&fresh(), "2026-11-02", "2026-10-30"), &people(&["a", "b", "c", "d", "e"])),
            &s_keys,
        );
        assert_eq!(pile(&st3, 479), 0);
        assert!(pile(&st3, 1020) >= 10);
        let w = washed(&st3, 810, 0);
        assert_eq!(pile(&w, 811), 0);
        assert!(due("plants", 540) && !due("plants", 480));
        assert!(!due("cups", 700) && due("cups", 815));
        assert!(due("trash", 1020) && !due("trash", 1110));
    }
}
