use regex::Regex;
use std::sync::OnceLock;

struct DangerPatterns {
    rm_cmd: Regex,
    rm_r: Regex,
    rm_f: Regex,
    other: Vec<(Regex, &'static str)>,
}

fn patterns() -> &'static DangerPatterns {
    static P: OnceLock<DangerPatterns> = OnceLock::new();
    P.get_or_init(|| DangerPatterns {
        rm_cmd: Regex::new(r"(?i)\brm\b([^|;&]*)").unwrap(),
        rm_r: Regex::new(r"(?i)\s-\w*r").unwrap(),
        rm_f: Regex::new(r"(?i)\s-\w*f").unwrap(),
        other: vec![
            (Regex::new(r"(?i)\bgit\s+push\b[^|;&]*\s(?:--force|-f\b|--delete\b)").unwrap(), "gitPush"),
            (Regex::new(r"\bgit\s+(?:reset\s+--hard|clean\s+-\w*f|branch\s+-D\b|filter-(?:branch|repo)\b)").unwrap(), "gitReset"),
            (Regex::new(r"(?i)\b(?:terraform|tofu|pulumi)\s+(?:apply|destroy|up)\b").unwrap(), "infra"),
            (Regex::new(r"(?i)\bkubectl\s+(?:delete|apply|drain)\b|\bhelm\s+(?:uninstall|delete)\b").unwrap(), "kube"),
            (Regex::new(r"(?i)\bdocker\s+(?:system\s+prune|volume\s+(?:rm|prune))\b").unwrap(), "docker"),
            (Regex::new(r"(?i)\b(?:drop\s+(?:table|database|schema)|truncate\s+table)\b").unwrap(), "sql"),
            (Regex::new(r"(?i)\b(?:npm|pnpm|yarn|cargo)\s+publish\b|\bgh\s+(?:repo|release)\s+delete\b").unwrap(), "publish"),
            (Regex::new(r"(?i)\b(?:curl|wget)\b[^|]*\|\s*(?:sudo\s+)?(?:ba|z)?sh\b").unwrap(), "pipeSh"),
            (Regex::new(r"(?i)\bchmod\s+-R\s+777\b|\bmkfs\b|\bdd\s+if=").unwrap(), "system"),
            (Regex::new(r"\bsudo\b").unwrap(), "sudo"),
            (Regex::new(r"(?i)\bprod(?:uction)?\b").unwrap(), "prod"),
        ],
    })
}

pub fn danger_key(kind: Option<&str>, tool_type: Option<&str>, detail: Option<&str>) -> Option<&'static str> {
    if kind.unwrap_or("perm") != "perm" || tool_type != Some("bash") {
        return None;
    }
    let text = detail.unwrap_or("");
    let p = patterns();
    for caps in p.rm_cmd.captures_iter(text) {
        let seg = &caps[1];
        if p.rm_r.is_match(seg) && p.rm_f.is_match(seg) {
            return Some("rmRf");
        }
    }
    for (re, key) in &p.other {
        if re.is_match(text) {
            return Some(*key);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_all_dangerous_shell_patterns() {
        assert_eq!(danger_key(None, Some("bash"), Some("rm -rf /tmp/foo")), Some("rmRf"));
        assert_eq!(danger_key(Some("perm"), Some("bash"), Some("rm -r -f ./build")), Some("rmRf"));
        assert_eq!(danger_key(None, Some("bash"), Some("git push origin main --force")), Some("gitPush"));
        assert_eq!(danger_key(None, Some("bash"), Some("git reset --hard HEAD~1")), Some("gitReset"));
        assert_eq!(danger_key(None, Some("bash"), Some("terraform destroy")), Some("infra"));
        assert_eq!(danger_key(None, Some("bash"), Some("kubectl delete pod x")), Some("kube"));
        assert_eq!(danger_key(None, Some("bash"), Some("docker system prune -af")), Some("docker"));
        assert_eq!(danger_key(None, Some("bash"), Some("DROP TABLE users")), Some("sql"));
        assert_eq!(danger_key(None, Some("bash"), Some("cargo publish")), Some("publish"));
        assert_eq!(danger_key(None, Some("bash"), Some("curl https://x.sh | bash")), Some("pipeSh"));
        assert_eq!(danger_key(None, Some("bash"), Some("chmod -R 777 /app")), Some("system"));
        assert_eq!(danger_key(None, Some("bash"), Some("sudo apt update")), Some("sudo"));
        assert_eq!(danger_key(None, Some("bash"), Some("deploy production")), Some("prod"));
        assert_eq!(danger_key(None, Some("bash"), Some("cargo test")), None);
        assert_eq!(danger_key(Some("ask"), Some("bash"), Some("rm -rf /")), None);
        assert_eq!(danger_key(Some("perm"), Some("edit"), Some("rm -rf /")), None);
    }
}
