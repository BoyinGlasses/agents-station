use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickChoice {
    pub key: String,
    pub label: String,
    pub reply: String,
}

fn opt_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:[-*]\s+)?\**(\d{1,2}|[A-Za-z])\**[.)]\**\s+(.+?)\s*$").unwrap()
    })
}

pub fn quick_choices(text: &str) -> Vec<QuickChoice> {
    let re = opt_re();
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in trimmed.split('\n').rev() {
        let Some(caps) = re.captures(line) else {
            break;
        };
        let key = caps[1].to_owned();
        let label: String = caps[2].replace('*', "").chars().take(80).collect();
        out.push(QuickChoice {
            reply: key.clone(),
            key,
            label,
        });
    }
    out.reverse();
    if !(2..=5).contains(&out.len()) {
        return Vec::new();
    }
    let first = &out[0].key;
    let is_num = first.bytes().all(|b| b.is_ascii_digit());
    let ok = out.iter().enumerate().all(|(i, o)| {
        if is_num {
            let start = first.parse::<usize>().ok();
            start.map(|s| o.key.to_ascii_lowercase() == (s + i).to_string()).unwrap_or(false)
        } else {
            let start = first.to_ascii_lowercase().as_bytes()[0];
            let expected = (start.wrapping_add(i as u8) as char).to_string();
            o.key.to_ascii_lowercase() == expected
        }
    });
    if ok { out } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lettered_options_at_end_become_choices() {
        let c = quick_choices("Làm gì tiếp?\nA) Push lên origin\nB) Xoá tệp tạm\nC. Bỏ qua");
        let replies: Vec<&str> = c.iter().map(|x| x.reply.as_str()).collect();
        assert_eq!(replies, vec!["A", "B", "C"]);
        assert_eq!(c[0].label, "Push lên origin");
    }

    #[test]
    fn numbered_and_bulleted_bold_options() {
        assert_eq!(quick_choices("Chọn:\n1. một\n2. hai").len(), 2);
        assert_eq!(quick_choices("Chọn:\n- **1)** một\n- **2)** hai").len(), 2);
    }

    #[test]
    fn rejects_non_choice_lists() {
        assert!(quick_choices("xong rồi").is_empty());
        assert!(quick_choices("1. chỉ một").is_empty());
        assert!(quick_choices("A) x\nB) y\nrồi hỏi thêm một câu cuối").is_empty());
        assert!(quick_choices("1. a\n3. b").is_empty());
        assert!(quick_choices("").is_empty());
    }
}
