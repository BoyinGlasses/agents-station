use crate::shared::prompt_coach::coach;
use crate::store::vault::VaultPrompt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

pub const HINTS: [&str; 5] = ["photoBare", "dangling", "vague", "multi", "tooShort"];

fn skip_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)^\s*(?:/|\[|<)|^\s*(?:ok(?:e|ay|ie)?|yes|no|có|không|ừ|uh|ờ|được|dc|đc|tiếp|tiếp tục|continue|go|làm đi|làm tiếp|\d{1,2}|[a-e])\s*[.!]*\s*$").unwrap()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptReportExample {
    pub t: String,
    pub ph: usize,
    pub hint: String,
    pub phrase: String,
    pub n: usize,
    pub at: u64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptReport {
    pub n: usize,
    pub clear: usize,
    pub pct: Option<u32>,
    pub convs: usize,
    pub by: BTreeMap<String, usize>,
    pub examples: Vec<PromptReportExample>,
}

pub fn build_prompt_report(prompts: &[VaultPrompt], max_examples: usize) -> PromptReport {
    let mut by: BTreeMap<String, usize> = HINTS.iter().map(|&h| (h.to_owned(), 0)).collect();
    let mut worst: Vec<(&VaultPrompt, &'static str, String, usize)> = Vec::new();
    let mut n = 0usize;
    let mut clear = 0usize;
    let mut convs: HashSet<&str> = HashSet::new();
    let skip = skip_re();

    for p in prompts {
        if p.t.is_empty() && p.ph == 0 {
            continue;
        }
        if p.ph == 0 && skip.is_match(&p.t) {
            continue;
        }
        n += 1;
        convs.insert(&p.sid);
        let hints = coach(&p.t, p.ph, false);
        if hints.is_empty() {
            clear += 1;
            continue;
        }
        for h in &hints {
            *by.entry(h.id.to_owned()).or_insert(0) += 1;
        }
        let first = &hints[0];
        worst.push((
            p,
            first.id,
            first.phrase.clone().unwrap_or_default(),
            first.n.unwrap_or(0),
        ));
    }

    let mut order: Vec<&str> = HINTS
        .iter()
        .copied()
        .filter(|h| by.get(*h).copied().unwrap_or(0) > 0)
        .collect();
    order.sort_by(|a, b| by.get(*b).cmp(&by.get(*a)));

    let mut examples = Vec::new();
    for kind in order {
        if examples.len() >= max_examples {
            break;
        }
        if let Some((p, hid, phrase, cnt)) = worst.iter().rev().find(|(_, k, _, _)| *k == kind) {
            let clipped = if p.t.chars().count() > 240 {
                let mut s: String = p.t.chars().take(239).collect();
                s.push('…');
                s
            } else {
                p.t.clone()
            };
            examples.push(PromptReportExample {
                t: clipped,
                ph: p.ph,
                hint: (*hid).to_owned(),
                phrase: phrase.clone(),
                n: *cnt,
                at: p.at,
                title: p.title.clone(),
            });
        }
    }

    let pct = if n > 0 {
        Some(((clear as f64 / n as f64) * 100.0).round() as u32)
    } else {
        None
    };

    PromptReport {
        n,
        clear,
        pct,
        convs: convs.len(),
        by,
        examples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_report_scores_clear_and_flags_habits() {
        let prompts = vec![
            VaultPrompt { t: "ok".into(), ph: 0, at: 1, sid: "s1".into(), title: "t".into() },
            VaultPrompt { t: "khác đi".into(), ph: 0, at: 2, sid: "s1".into(), title: "t".into() },
            VaultPrompt { t: "Thêm nút \"Xuất CSV\" vào trang báo cáo, cột ngày dd/mm/yyyy".into(), ph: 0, at: 3, sid: "s2".into(), title: "t2".into() },
        ];
        let rep = build_prompt_report(&prompts, 3);
        assert_eq!(rep.n, 2);
        assert_eq!(rep.clear, 1);
        assert_eq!(rep.pct, Some(50));
        assert_eq!(rep.by["vague"], 1);
        assert_eq!(rep.examples.len(), 1);
    }
}
