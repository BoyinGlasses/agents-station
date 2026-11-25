use regex::Regex;
use std::sync::OnceLock;

pub const SHOT_ASK: &str = "You are being written to from My Office (the user is on a phone). If your work changed a web page running on this computer (a dev server already running), end your answer with one line per page worth seeing, at most 3, exactly like: SCREENSHOT: http://localhost:3000/the/page . The office photographs them for the phone. Do not start a server for this.";

pub fn is_local_shot_url(raw: &str) -> bool {
    let low = raw.trim().to_ascii_lowercase();
    let rest = low
        .strip_prefix("http://")
        .or_else(|| low.strip_prefix("https://"));
    let Some(r) = rest else {
        return false;
    };
    r.starts_with("localhost:")
        || r.starts_with("localhost/")
        || r == "localhost"
        || r.starts_with("127.0.0.1:")
        || r.starts_with("127.0.0.1/")
        || r == "127.0.0.1"
        || r.starts_with("[::1]:")
        || r.starts_with("[::1]/")
        || r == "[::1]"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedShots {
    pub text: String,
    pub urls: Vec<String>,
}

pub fn shots_in(text: &str) -> ExtractedShots {
    static LINE_RE: OnceLock<Regex> = OnceLock::new();
    let re = LINE_RE.get_or_init(|| Regex::new(r"(?im)^\s*SCREENSHOT:\s*(\S+)\s*$").unwrap());
    let mut urls = Vec::new();
    for caps in re.captures_iter(text) {
        let u = caps[1].to_owned();
        if is_local_shot_url(&u) && !urls.contains(&u) {
            urls.push(u);
            if urls.len() >= 3 {
                break;
            }
        }
    }
    let stripped = re.replace_all(text, "");
    let cleaned = stripped
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    ExtractedShots {
        text: cleaned,
        urls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shots_in_extracts_only_loopback_urls_up_to_three() {
        let s = shots_in(
            "Done!\nSCREENSHOT: http://localhost:3000/home\nSCREENSHOT: https://evil.com/leak\nSCREENSHOT: http://127.0.0.1:4310/",
        );
        assert_eq!(s.text, "Done!");
        assert_eq!(
            s.urls,
            vec!["http://localhost:3000/home", "http://127.0.0.1:4310/"]
        );
    }
}
