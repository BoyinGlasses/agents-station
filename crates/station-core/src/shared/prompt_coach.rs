use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub k: String,
    pub p: String,
    pub bare: bool,
    pub at: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSlice {
    pub text: String,
    pub kind: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoachHint {
    pub id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phrase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<usize>,
}

fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.nfd() {
        let cp = ch as u32;
        if (0x0300..=0x036F).contains(&cp) {
            continue;
        }
        if ch == 'đ' {
            out.push('d');
        } else {
            out.push(ch);
        }
    }
    out
}

fn squeeze(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev: Option<char> = None;
    for ch in s.chars() {
        if ch.is_alphabetic() && prev == Some(ch) {
            continue;
        }
        out.push(ch);
        prev = Some(ch);
    }
    out
}

fn key_of(w: &str) -> String {
    let nfc: String = w.nfc().collect();
    squeeze(&nfc.to_lowercase())
}

fn same(tok: &Token, word: &str) -> bool {
    tok.k == word || (tok.bare && tok.p == plain(word))
}

struct CoachData {
    short: HashMap<&'static str, &'static str>,
    filler: Vec<String>,
    done: Vec<String>,
    vague: Vec<Vec<String>>,
    follow: Vec<String>,
    before: Vec<String>,
    criteria: Vec<Vec<String>>,
    dangling: Vec<Vec<String>>,
    numbered: Vec<Vec<String>>,
    unit: Vec<String>,
    verbs: Vec<(&'static str, Vec<Vec<String>>)>,
    all_verbs: Vec<Vec<String>>,
    strong: Vec<Vec<String>>,
    reverse: Vec<Vec<String>>,
    weak: Vec<Vec<String>>,
    tok_re: Regex,
    anchor_re: Regex,
    single_quote_re: Regex,
    criterion_re: Regex,
    leading_num_re: Regex,
    list_two_re: Regex,
    comma_re: Regex,
}

fn words(s: &str) -> Vec<String> {
    s.split(' ').map(key_of).collect()
}

fn phrases(list: &[&str]) -> Vec<Vec<String>> {
    list.iter()
        .map(|s| s.split(' ').map(key_of).collect())
        .collect()
}

fn data() -> &'static CoachData {
    static DATA: OnceLock<CoachData> = OnceLock::new();
    DATA.get_or_init(|| {
        let short: HashMap<&'static str, &'static str> = [
            ("j", "gì"), ("z", "vậy"), ("dz", "vậy"), ("ko", "không"), ("k", "không"),
            ("hok", "không"), ("khum", "không"), ("kh", "không"), ("dc", "được"), ("đc", "được"),
            ("r", "rồi"), ("s", "sao"), ("vs", "với"), ("ntn", "như thế nào"), ("bt", "biết"),
            ("cx", "cũng"), ("ms", "mới"), ("mn", "mọi người"), ("nài", "này"), ("nì", "này"),
            ("kái", "cái"), ("đy", "đi"), ("dy", "đi"), ("oke", "ok"), ("okie", "ok"),
            ("okay", "ok"), ("okela", "ok"), ("pls", "please"), ("plz", "please"),
        ]
        .into_iter()
        .collect();

        let verbs: Vec<(&'static str, Vec<Vec<String>>)> = vec![
            ("ship", phrases(&["push", "deploy", "commit", "merge", "release", "publish", "đẩy lên", "đẩy code", "up lên", "lên github", "tạo pr", "mở pr", "ship"])),
            ("think", phrases(&["gợi ý", "đề xuất", "giải thích", "phân tích", "review", "so sánh", "tìm hiểu", "nghiên cứu", "đánh giá", "tư vấn", "lên kế hoạch", "lập kế hoạch", "nhận xét", "cho ý kiến", "brainstorm", "suggest", "explain", "analyse", "analyze", "compare", "research", "propose", "investigate", "plan"])),
            ("change", phrases(&["sửa", "thêm", "xoá", "xóa", "bỏ", "đổi", "thay", "viết", "tạo", "cập nhật", "làm", "chỉnh", "dời", "chuyển", "gộp", "tách", "cài", "dịch", "fix", "add", "remove", "delete", "change", "replace", "write", "create", "update", "refactor", "implement", "rename", "move", "make", "install", "translate"])),
            ("check", phrases(&["test", "chạy test", "chạy", "kiểm tra", "check", "verify", "chụp", "xem lại", "đo", "run", "lint", "build"])),
        ];
        let all_verbs: Vec<Vec<String>> = verbs.iter().flat_map(|(_, l)| l.clone()).collect();

        CoachData {
            short,
            filler: words("sao này cái đó đây kia đấy nữa nhé nhe nha ạ à ơi đi vậy thế gì nào hả hở chứ luôn với giúp dùm giùm hộ xem coi check fix help em anh chị mình tôi tao bạn sếp cho ra lại thử được không có rồi là thì mà ok cũng như thế ở trong please the this that it what why how here there now again do can you i me a an to is of on in pls ?"),
            done: words("ok oki rồi đi luôn nhé nhe nha ạ dạ vâng ừ ừm uh với yes yep yeah no nope có không đúng sai được xong push deploy commit merge rebase tag release continue go ahead on tiếp tục làm lại thử chạy test tests build lint sửa fix revert undo retry run it the again now please do that dừng stop huỷ hủy cancel cảm ơn cám thanks thank you thx tks lgtm đồng ý duyệt approve ship cả hai 2 both all tất phương án option chọn số pick lên github main pr"),
            vague: phrases(&[
                "khác đi", "khác hơn", "khác khác", "khác chút", "khác đy", "hay hơn", "hay lên", "hay ho hơn",
                "đẹp hơn", "đẹp lên", "đẹp đẹp", "tốt hơn", "xịn hơn", "xịn lên", "xịn xò", "ngon hơn", "chất hơn",
                "ổn hơn", "sang hơn", "pro hơn", "đỉnh hơn", "chuyên nghiệp hơn", "hiện đại hơn", "chỉn chu hơn",
                "tinh tế hơn", "ấn tượng hơn", "bắt mắt hơn", "hấp dẫn hơn", "thu hút hơn", "giống thật", "như thật",
                "thật hơn", "qua loa", "làm cho có", "cẩu thả", "làm ẩu", "đàng hoàng", "tử tế", "xấu quá", "dở quá",
                "chán quá", "chưa ổn", "chưa đẹp", "không ổn", "make it better", "do better", "look better",
                "looks better", "nicer", "prettier", "more professional", "more modern", "more polished", "fancier",
                "cooler", "more real", "look real", "looks real", "more realistic", "something different",
                "make it different", "not lazy", "sloppy", "improve it", "polish it", "make it pop", "not good",
                "looks bad", "looks off", "not right",
            ]),
            follow: words("là nếu nên bằng như kiểu giống theo với than by like with using rồi nhiều hẳn now already"),
            before: words("đã thấy"),
            criteria: phrases(&[
                "màu", "xanh", "vàng", "tím", "hồng", "xám", "trắng", "font", "phông", "chữ", "cỡ", "size", "nền",
                "background", "layout", "bố cục", "icon", "bóng", "shadow", "viền", "border", "hiệu ứng", "animation",
                "to hơn", "nhỏ hơn", "lớn hơn", "đậm hơn", "nhạt hơn", "sáng hơn", "tối hơn", "rộng hơn", "hẹp hơn",
                "cao hơn", "thấp hơn", "nhanh hơn", "chậm hơn", "ngắn hơn", "dài hơn", "ít hơn", "nhiều hơn", "gọn hơn",
                "thưa hơn", "sát hơn", "ví dụ", "vd", "tiêu chí", "cụ thể", "chẳng hạn", "giống như", "kiểu như",
                "tham khảo", "bigger", "smaller", "larger", "bolder", "darker", "lighter", "faster", "slower",
                "shorter", "longer", "wider", "narrower", "color", "colour", "spacing", "padding", "margin",
                "for example", "such as", "e g", "eg", "criteria", "specifically",
            ]),
            dangling: phrases(&[
                "cái này", "cái đó", "cái đấy", "cái kia", "cái ấy", "chỗ này", "chỗ đó", "chỗ đấy", "chỗ kia",
                "đoạn này", "đoạn đó", "đoạn trên", "phần này", "phần đó", "như trên", "ở trên", "bên trên",
                "như lúc nãy", "lúc nãy", "hồi nãy", "vừa nãy", "this one", "that one", "this thing", "that thing",
                "the above", "as above", "same as before", "like before", "the previous one", "that part",
            ]),
            numbered: phrases(&["số", "ý", "mục", "câu", "bước", "cái thứ", "item", "number", "point", "step"]),
            unit: words("px phút giây s ms ngày tiếng giờ lần người k tr triệu nghìn ngàn đồng file dòng trang cột hàng màu cái chiếc con bài câu nút lỗi bug test task việc ảnh hình chỗ tab màn min mins sec days hours times em rem kb mb gb"),
            verbs,
            all_verbs,
            strong: phrases(&["trước đó", "trước tiên", "trước hết", "sau đó", "rồi sau đó", "và sau đó", "xong thì", "xong rồi", "xong rồi thì", "rồi thì", "then", "and then", "and also", "after that", "afterwards", "before that", "but first"]),
            reverse: phrases(&["trước đó", "trước tiên", "trước hết", "before that", "but first"]),
            weak: phrases(&["rồi", "và", "xong", "với lại", "thêm nữa", "đồng thời", "ngoài ra", "and", "also", "plus"]),
            tok_re: Regex::new(r"[\p{L}\p{M}\p{N}]+").unwrap(),
            anchor_re: Regex::new(r#"(?i)["“«‘`][^"”»’`\n]{2,}["”»’`]|\b[\w-]+\.(?:[cm]?[jt]sx?|css|scss|html?|py|md|json|ya?ml|sh|go|rs|java|kt|swift|rb|php|sql|vue|svelte|txt|csv|env|png|jpe?g)\b|(?:^|\s)(?:~|\.{1,2})?/[\w.-]+/[\w./-]*|https?://\S+"#).unwrap(),
            single_quote_re: Regex::new(r"(?:^|\s)'[^'\n]{2,}'(?:$|[\s.,!?;:])").unwrap(),
            criterion_re: Regex::new(r"(?i)\d|#[0-9a-f]{3,8}\b|:\s*\S+\s+\S+|\n\s*(?:[-*•]|\d+[.)])\s").unwrap(),
            leading_num_re: Regex::new(r"^\s*\d{1,2}\s+\p{L}").unwrap(),
            list_two_re: Regex::new(r"\n\s*2\s*[.):-]?\s+\p{L}").unwrap(),
            comma_re: Regex::new(r"[,…]\s*$").unwrap(),
        }
    })
}

pub fn tokens(text: &str) -> Vec<Token> {
    let d = data();
    let nfc: String = text.nfc().collect();
    let mut out = Vec::new();
    for m in d.tok_re.find_iter(&nfc) {
        let k = key_of(m.as_str());
        let at = m.start();
        let end = m.end();
        let expanded = d.short.get(k.as_str()).copied();
        let source = expanded.unwrap_or(k.as_str());
        let bare = plain(&k) == k && expanded.is_none();
        for w in source.split(' ') {
            let kk = key_of(w);
            let p = plain(&kk);
            out.push(Token { k: kk, p, bare, at, end });
        }
    }
    out
}

fn phrase_at<'a>(toks: &[Token], i: usize, list: &'a [Vec<String>]) -> Option<&'a Vec<String>> {
    let mut best: Option<&'a Vec<String>> = None;
    for ph in list {
        if ph.len() <= toks.len() - i
            && ph.iter().enumerate().all(|(j, w)| same(&toks[i + j], w))
            && best.is_none_or(|b| ph.len() > b.len())
        {
            best = Some(ph);
        }
    }
    best
}

fn find_phrase(toks: &[Token], list: &[Vec<String>], from: usize) -> Option<(usize, usize)> {
    for i in from..toks.len() {
        if let Some(ph) = phrase_at(toks, i, list) {
            return Some((i, ph.len()));
        }
    }
    None
}

fn in_list(tok: &Token, list: &[String]) -> bool {
    list.iter().any(|w| same(tok, w))
}

fn starts_job(toks: &[Token], i: usize) -> bool {
    let d = data();
    i < toks.len()
        && (phrase_at(toks, i, &d.all_verbs).is_some()
            || (i + 1 < toks.len()
                && in_list(&toks[i], &d.filler)
                && phrase_at(toks, i + 1, &d.all_verbs).is_some()))
}

fn verb_kind_of(ph: &[String]) -> Option<&'static str> {
    let d = data();
    for (kind, list) in &d.verbs {
        if list.iter().any(|p| p.as_slice() == ph) {
            return Some(*kind);
        }
    }
    None
}

fn split_sentences(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut iter = src.char_indices().peekable();
    while let Some((idx, ch)) = iter.next() {
        let is_sep = match ch {
            '\n' | '!' | '?' | ';' => true,
            '.' => match iter.peek() {
                Some(&(_, next_ch)) => next_ch.is_whitespace(),
                None => true,
            },
            _ => false,
        };
        if is_sep {
            if idx > start {
                out.push(&src[start..idx]);
            }
            start = idx + ch.len_utf8();
        }
    }
    if start < src.len() {
        out.push(&src[start..]);
    }
    out
}

pub fn split_tasks(text: &str) -> Vec<TaskSlice> {
    let d = data();
    let src: String = text.nfc().collect();
    struct Piece {
        text: String,
        reverse: bool,
        toks: Vec<Token>,
    }
    let mut out: Vec<Piece> = Vec::new();

    for sent in split_sentences(&src) {
        let toks = tokens(sent);
        let mut from = 0usize;
        let mut start_tok = 0usize;
        let mut reverse = false;

        let mut cut = |end_tok: usize, to: usize, next_from: usize, next_tok: usize, rev: bool| {
            let raw = &sent[from..to];
            let piece = raw.trim_matches(|c: char| {
                c.is_whitespace() || matches!(c, ',' | '…' | ':' | '–' | '-')
            });
            if !piece.is_empty() {
                out.push(Piece {
                    text: piece.to_owned(),
                    reverse,
                    toks: toks[start_tok..end_tok].to_vec(),
                });
            }
            from = next_from;
            start_tok = next_tok;
            reverse = rev;
        };

        let mut i = 1usize;
        while i < toks.len() {
            let strong = phrase_at(&toks, i, &d.strong);
            let weak = if strong.is_none() {
                phrase_at(&toks, i, &d.weak)
            } else {
                None
            };
            let comma = strong.is_none()
                && weak.is_none()
                && d.comma_re.is_match(&sent[toks[i - 1].end..toks[i].at])
                && starts_job(&toks, i);
            let ph = strong.or(weak);
            if ph.is_none() && !comma {
                i += 1;
                continue;
            }
            let n = ph.map(|p| p.len()).unwrap_or(0);
            if weak.is_some() && !starts_job(&toks, i + n) {
                i += 1;
                continue;
            }
            let next_from = if n > 0 { toks[i + n - 1].end } else { toks[i].at };
            let is_rev = strong.is_some()
                && phrase_at(&toks, i, &d.reverse).is_some_and(|rp| rp.len() == n);
            cut(i, toks[i].at, next_from, i + n, is_rev);
            i += n.max(1);
        }
        cut(toks.len(), sent.len(), sent.len(), toks.len(), false);
    }

    let mut ordered: Vec<Piece> = Vec::with_capacity(out.len());
    for p in out {
        if p.reverse && !ordered.is_empty() {
            let idx = ordered.len() - 1;
            ordered.insert(idx, p);
        } else {
            ordered.push(p);
        }
    }

    ordered
        .into_iter()
        .map(|p| {
            let kind = (0..p.toks.len())
                .find_map(|i| phrase_at(&p.toks, i, &d.all_verbs).and_then(|ph| verb_kind_of(ph)));
            TaskSlice { text: p.text, kind }
        })
        .collect()
}

pub fn number_tasks(text: &str, head: &str) -> String {
    let jobs = split_tasks(text);
    if jobs.len() < 2 {
        return text.to_owned();
    }
    let lines: Vec<String> = jobs
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let mut chars = j.text.chars();
            let capped = match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            };
            format!("{}. {capped}", i + 1)
        })
        .collect();
    if head.is_empty() {
        lines.join("\n")
    } else {
        format!("{head}\n{}", lines.join("\n"))
    }
}

fn has_anchor(src: &str) -> bool {
    let d = data();
    d.anchor_re.is_match(src) || d.single_quote_re.is_match(src)
}

pub fn coach(text: &str, photos: usize, quoted: bool) -> Vec<CoachHint> {
    let d = data();
    let src: String = text.nfc().collect();
    if src.trim_start().starts_with('/') {
        return Vec::new();
    }
    let len = src.chars().filter(|c| c.is_alphanumeric()).count();
    if photos == 0 && len == 0 {
        return Vec::new();
    }
    let toks = tokens(&src);
    let content: Vec<&Token> = toks.iter().filter(|t| !in_list(t, &d.filler)).collect();
    let said = |i: usize, n: usize| -> String { src[toks[i].at..toks[i + n - 1].end].to_owned() };

    let mut out: Vec<CoachHint> = Vec::new();
    if photos > 0 && content.len() <= 2 && len < 25 {
        out.push(CoachHint { id: "photoBare", phrase: None, n: None });
    }

    let anchored = photos > 0 || quoted || has_anchor(&src);
    if !anchored {
        let mut hit = find_phrase(&toks, &d.dangling, 0);
        if hit.is_none() {
            for i in 0..toks.len().saturating_sub(2) {
                if same(&toks[i], "phần") && same(&toks[i + 1], "của") {
                    hit = Some((i, 3));
                    break;
                }
            }
        }
        if hit.is_none() {
            for i in 0..toks.len() {
                if let Some(ph) = phrase_at(&toks, i, &d.numbered) {
                    let j = i + ph.len();
                    if let Some(num_tok) = toks.get(j)
                        && (1..=2).contains(&num_tok.k.len())
                        && num_tok.k.bytes().all(|b| b.is_ascii_digit())
                        && toks.len().saturating_sub(ph.len() + 1) >= 4
                    {
                        hit = Some((i, ph.len() + 1));
                        break;
                    }
                }
            }
        }
        if hit.is_none()
            && d.leading_num_re.is_match(&src)
            && toks.len() >= 5
            && !in_list(&toks[1], &d.unit)
            && !d.list_two_re.is_match(&src)
        {
            hit = Some((0, 1));
        }
        if let Some((i, n)) = hit {
            out.push(CoachHint {
                id: "dangling",
                phrase: Some(said(i, n)),
                n: None,
            });
        }
    }

    let mut vague_hits = Vec::new();
    let mut cursor = 0usize;
    while let Some((i, n)) = find_phrase(&toks, &d.vague, cursor) {
        let next = toks.get(i + n);
        let prev = i.checked_sub(1).and_then(|p| toks.get(p));
        let after = src[toks[i + n - 1].end..].trim_start();
        let next_follow = next.is_some_and(|t| in_list(t, &d.follow));
        let prev_before = prev.is_some_and(|t| in_list(t, &d.before));
        if !next_follow && !prev_before && !after.starts_with(':') {
            vague_hits.push((i, n));
        }
        cursor = i + n;
    }
    if !vague_hits.is_empty() {
        let used: HashSet<usize> = vague_hits
            .iter()
            .flat_map(|&(i, n)| i..(i + n))
            .collect();
        let rest_count = toks
            .iter()
            .enumerate()
            .filter(|(idx, t)| !used.contains(idx) && !in_list(t, &d.filler))
            .count();
        if !d.criterion_re.is_match(&src)
            && !has_anchor(&src)
            && find_phrase(&toks, &d.criteria, 0).is_none()
            && rest_count < 10
        {
            let (i, n) = vague_hits[0];
            out.push(CoachHint {
                id: "vague",
                phrase: Some(said(i, n)),
                n: None,
            });
        }
    }

    let jobs: Vec<TaskSlice> = split_tasks(&src).into_iter().filter(|j| j.kind.is_some()).collect();
    let kinds: HashSet<&str> = jobs.iter().filter_map(|j| j.kind).collect();
    if jobs.len() >= 3
        || (jobs.len() == 2
            && kinds.len() >= 2
            && (content.len() >= 6 || find_phrase(&toks, &d.reverse, 0).is_some()))
    {
        out.push(CoachHint {
            id: "multi",
            phrase: None,
            n: Some(jobs.len()),
        });
    }

    if out.is_empty()
        && photos == 0
        && src.trim().chars().count() < 12
        && !toks.iter().all(|t| {
            in_list(t, &d.done)
                || ((1..=2).contains(&t.k.len()) && t.k.bytes().all(|b| b.is_ascii_digit()))
                || (t.k.len() == 1 && matches!(t.k.as_bytes()[0], b'a'..=b'e'))
        })
    {
        out.push(CoachHint { id: "tooShort", phrase: None, n: None });
    }

    let prio = |id: &str| match id {
        "photoBare" => 0,
        "dangling" => 1,
        "vague" => 2,
        "multi" => 3,
        "tooShort" => 4,
        _ => 5,
    };
    out.sort_by_key(|h| prio(h.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(text: &str, photos: usize, quoted: bool) -> Vec<&'static str> {
        coach(text, photos, quoted).into_iter().map(|h| h.id).collect()
    }

    #[test]
    fn photo_bare_matches_oracle() {
        for t in ["sao", "điền sao", "config gì", "cái này nữa nhé", "đây", "fix", "dien sao", "config j", "cai nay nua nhe", "Sao?", "why", ""] {
            assert_eq!(coach(t, 1, false).first().map(|h| h.id), Some("photoBare"), "{t}");
        }
        for t in ["đổi thành map vệ tinh như ảnh", "Sửa lỗi đang thấy trong ảnh: sao", "nút đăng nhập bị lệch sang phải như ảnh, căn giữa lại", "make the header look like this screenshot"] {
            assert!(ids(t, 1, false).is_empty(), "{t}");
        }
    }

    #[test]
    fn vague_matches_oracle() {
        let cases = [
            ("khác đi", "khác đi"), ("hay hơn", "hay hơn"), ("làm đẹp hơn đi", "đẹp hơn"),
            ("tốt hơn nữa", "tốt hơn"), ("xịn hơn chút", "xịn hơn"),
            ("làm giống thật đừng làm qua loa", "giống thật"), ("chuyên nghiệp hơn", "chuyên nghiệp hơn"),
            ("khac di", "khac di"), ("lam giong that dung lam qua loa", "giong that"),
            ("đẹp hơnnn", "đẹp hơnnn"), ("qua loa quá", "qua loa"), ("make it better", "make it better"),
            ("looks off, more professional please", "looks off"),
        ];
        for (t, phrase) in cases {
            let h = coach(t, 0, false).into_iter().next().unwrap_or_else(|| panic!("no hint for {t}"));
            assert_eq!(h.id, "vague", "{t}");
            assert_eq!(h.phrase.as_deref(), Some(phrase), "{t}");
        }
        for t in ["đẹp hơn: nền tối, nút bo tròn", "khác đi nhé, kiểu nền tối chữ to", "hay hơn như trang của Stripe", "tốt hơn là dùng sqlite", "làm đẹp hơn, chữ to hơn và màu xanh lá", "make it better: bigger buttons, darker background", "lazy load ảnh ở trang chủ", "it's fine, don't change the colour", "đẹp hơn rồi, giờ thêm nút xuất CSV", "giao diện đã đẹp hơn, giờ sửa nút đăng nhập", "looks better now, ship it"] {
            assert!(!ids(t, 0, false).contains(&"vague"), "{t}");
        }
    }

    #[test]
    fn dangling_multi_and_too_short_match_oracle() {
        for (t, phrase) in [
            ("4 đâu có xem giải thích gì đâu", "4"),
            ("phần của vĩ", "phần của vĩ"),
            ("cái này lỗi rồi", "cái này"),
            ("sửa như trên", "như trên"),
            ("ý 4 sai rồi, giải thích lại kỹ hơn đi", "ý 4"),
            ("cai nay loi roi", "cai nay"),
            ("kái nài lỗi", "kái nài"),
            ("this one is broken again after the deploy", "this one"),
        ] {
            let h = coach(t, 0, false).into_iter().next().unwrap_or_else(|| panic!("no hint for {t}"));
            assert_eq!(h.id, "dangling", "{t}");
            assert_eq!(h.phrase.as_deref(), Some(phrase), "{t}");
        }
        assert!(!ids("cái này lỗi rồi", 1, false).contains(&"dangling"));
        assert!(!ids("cái này lỗi rồi", 0, true).contains(&"dangling"));
        assert!(!ids("1 gởi lại tôi nên điền gì\n2. làm 2 và 3 nhé", 0, false).contains(&"dangling"));
        for t in ["cái này trong Header.tsx bị lỗi", "đổi “Đăng nhập” thành “Vào” ở cái này", "cái này: src/office/ui/chat.js", "2 cái nút bị lệch quá", "3 phút là đủ", "chọn số 4", "phương án 2"] {
            assert!(!ids(t, 0, false).contains(&"dangling"), "{t}");
        }

        assert_eq!(
            number_tasks("gợi ý thêm vài cách làm trang giá cho gọn, trước đó push lên github", "Làm lần lượt:"),
            "Làm lần lượt:\n1. Push lên github\n2. Gợi ý thêm vài cách làm trang giá cho gọn"
        );
        assert_eq!(
            number_tasks("sửa lỗi header rồi chạy test, xong thì commit và push lên main", ""),
            "1. Sửa lỗi header\n2. Chạy test\n3. Commit\n4. Push lên main"
        );
        for t in ["sao", "config gì", "fix this", "tại sao", "lỗi"] {
            assert_eq!(ids(t, 0, false), vec!["tooShort"], "{t}");
        }
        for t in ["oke", "ok", "push đi", "deploy", "continue", "tiếp", "tiep di", "làm đi", "commit", "có", "không", "ko", "oke r deploy", "thử lại", "2", "cả 2", "B", "lgtm", "/compact", "🙏", "cảm ơn", "run the tests", "sửa đi", "test lại", "dạ", "vâng"] {
            assert!(ids(t, 0, false).is_empty(), "{t}");
        }
        assert_eq!(ids("cái này khác đi", 0, false), vec!["dangling", "vague"]);
        assert_eq!(ids("khác đi", 1, false), vec!["photoBare", "vague"]);
    }
}
