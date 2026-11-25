use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestCounts {
    pub pass: u32,
    pub fail: u32,
}

struct RunnerPatterns {
    split: Regex,
    prefix: Regex,
    help: Regex,
    npm: Regex,
    js: Regex,
    e2e: Regex,
    node_test: Regex,
    deno_bun: Regex,
    pytest: Regex,
    py_mod: Regex,
    go_test: Regex,
    cargo: Regex,
    jvm: Regex,
    ruby_php: Regex,
    vendor_php: Regex,
    other_test: Regex,
    ctest: Regex,
    make: Regex,
}

fn runner_pats() -> &'static RunnerPatterns {
    static P: OnceLock<RunnerPatterns> = OnceLock::new();
    P.get_or_init(|| RunnerPatterns {
        split: Regex::new(r"\s*(?:&&|\|\||;|\n)\s*").unwrap(),
        prefix: Regex::new(r"^(?:(?:\w+=\S*|timeout\s+\S+|time|nice|command|exec|npx|pnpx|bunx|pnpm\s+(?:exec|dlx)|yarn\s+dlx|uv\s+run|poetry\s+run|pipenv\s+run|bundle\s+exec)\s+)+").unwrap(),
        help: Regex::new(r"(?:^|\s)(?:--help|-h|--version|--list|--collect-only)(?:\s|$)").unwrap(),
        npm: Regex::new(r"^(npm|pnpm|yarn|bun)\s+(?:run\s+)?(test(?::[\w:.-]+)?)(?:\s|$)").unwrap(),
        js: Regex::new(r"^(vitest|jest|mocha|ava|tap|karma|jasmine)(?:\s|$)").unwrap(),
        e2e: Regex::new(r"^(playwright|cypress)\s+(?:test|run)(?:\s|$)").unwrap(),
        node_test: Regex::new(r"^node\s+(?:\S+\s+)*?--test(?:[\s=]|$)").unwrap(),
        deno_bun: Regex::new(r"^(deno|bun)\s+test(?:\s|$)").unwrap(),
        pytest: Regex::new(r"^(?:py\.test|pytest)(?:\s|$)").unwrap(),
        py_mod: Regex::new(r"^python[\d.]*\s+-m\s+(pytest|unittest)(?:\s|$)").unwrap(),
        go_test: Regex::new(r"^go\s+test(?:\s|$)").unwrap(),
        cargo: Regex::new(r"^cargo\s+(test|nextest)(?:\s|$)").unwrap(),
        jvm: Regex::new(r"^(?:\./)?(mvnw?|gradlew?)\s+(?:\S+\s+)*?test(?:\s|$)").unwrap(),
        ruby_php: Regex::new(r"^(rspec|phpunit|pest)(?:\s|$)").unwrap(),
        vendor_php: Regex::new(r"^(?:vendor/bin/|\./vendor/bin/)(phpunit|pest)(?:\s|$)").unwrap(),
        other_test: Regex::new(r"^(php\s+artisan|rails|rake|mix|dotnet|swift|flutter|dart)\s+test(?:\s|$)").unwrap(),
        ctest: Regex::new(r"^ctest(?:\s|$)").unwrap(),
        make: Regex::new(r"^make\s+(test|check)(?:\s|$)").unwrap(),
    })
}

pub fn test_runner(command: &str) -> Option<String> {
    let p = runner_pats();
    for part in p.split.split(command) {
        let seg = p.prefix.replace(part.trim(), "");
        if seg.is_empty() || p.help.is_match(&seg) {
            continue;
        }
        if let Some(c) = p.npm.captures(&seg) {
            return Some(format!("{} {}", &c[1], &c[2]));
        }
        if let Some(c) = p.js.captures(&seg) {
            return Some(c[1].to_owned());
        }
        if let Some(c) = p.e2e.captures(&seg) {
            return Some(c[1].to_owned());
        }
        if p.node_test.is_match(&seg) {
            return Some("node --test".to_owned());
        }
        if let Some(c) = p.deno_bun.captures(&seg) {
            return Some(format!("{} test", &c[1]));
        }
        if p.pytest.is_match(&seg) {
            return Some("pytest".to_owned());
        }
        if let Some(c) = p.py_mod.captures(&seg) {
            return Some(c[1].to_owned());
        }
        if p.go_test.is_match(&seg) {
            return Some("go test".to_owned());
        }
        if let Some(c) = p.cargo.captures(&seg) {
            return Some(format!("cargo {}", &c[1]));
        }
        if let Some(c) = p.jvm.captures(&seg) {
            return Some(c[1].trim_end_matches('w').to_owned());
        }
        if let Some(c) = p.ruby_php.captures(&seg) {
            return Some(c[1].to_owned());
        }
        if let Some(c) = p.vendor_php.captures(&seg) {
            return Some(c[1].to_owned());
        }
        if let Some(c) = p.other_test.captures(&seg) {
            let head = c[1].split_whitespace().collect::<Vec<_>>().join(" ");
            return Some(format!("{head} test"));
        }
        if p.ctest.is_match(&seg) {
            return Some("ctest".to_owned());
        }
        if let Some(c) = p.make.captures(&seg) {
            return Some(format!("make {}", &c[1]));
        }
    }
    None
}

fn last_num(re: &Regex, text: &str) -> Option<u32> {
    re.captures_iter(text)
        .filter_map(|c| c.get(1)?.as_str().parse::<u32>().ok())
        .last()
}

pub fn test_counts(out: &str) -> Option<TestCounts> {
    struct CountRes {
        node_pass: Regex,
        node_fail: Regex,
        passed: Regex,
        failed: Regex,
        rspec_ex: Regex,
        rspec_fail: Regex,
        php_ok: Regex,
        php_all: Regex,
        php_fail: Regex,
        php_err: Regex,
        go_pass: Regex,
        go_fail: Regex,
    }
    static RE: OnceLock<CountRes> = OnceLock::new();
    let r = RE.get_or_init(|| CountRes {
        node_pass: Regex::new(r"(?m)^\s*(?:#|ℹ)\s*pass\s+(\d+)").unwrap(),
        node_fail: Regex::new(r"(?m)^\s*(?:#|ℹ)\s*fail\s+(\d+)").unwrap(),
        passed: Regex::new(r"(\d+)\s+(?:passed|passing)\b").unwrap(),
        failed: Regex::new(r"(\d+)\s+(?:failed|failing)\b").unwrap(),
        rspec_ex: Regex::new(r"(\d+)\s+examples?,").unwrap(),
        rspec_fail: Regex::new(r"(\d+)\s+failures?\b").unwrap(),
        php_ok: Regex::new(r"\bOK \((\d+) tests?").unwrap(),
        php_all: Regex::new(r"\bTests:\s*(\d+),").unwrap(),
        php_fail: Regex::new(r"\bFailures:\s*(\d+)").unwrap(),
        php_err: Regex::new(r"\bErrors:\s*(\d+)").unwrap(),
        go_pass: Regex::new(r"(?m)^\s*--- PASS").unwrap(),
        go_fail: Regex::new(r"(?m)^\s*--- FAIL").unwrap(),
    });

    let start = out.len().saturating_sub(20_000);
    let text = &out[out.ceil_char_boundary(start)..];

    let mut pass = last_num(&r.node_pass, text);
    let mut fail = last_num(&r.node_fail, text);
    if pass.is_none() && fail.is_none() {
        pass = last_num(&r.passed, text);
        fail = last_num(&r.failed, text);
    }
    if pass.is_none() && fail.is_none()
        && let Some(ex) = last_num(&r.rspec_ex, text)
    {
        let f = last_num(&r.rspec_fail, text).unwrap_or(0);
        pass = Some(ex.saturating_sub(f));
        fail = Some(f);
    }
    if pass.is_none() && fail.is_none() {
        if let Some(ok) = last_num(&r.php_ok, text) {
            pass = Some(ok);
            fail = Some(0);
        } else if let Some(all) = last_num(&r.php_all, text) {
            let f = last_num(&r.php_fail, text).unwrap_or(0) + last_num(&r.php_err, text).unwrap_or(0);
            pass = Some(all.saturating_sub(f));
            fail = Some(f);
        }
    }
    if pass.is_none() && fail.is_none() {
        let p = r.go_pass.find_iter(text).count() as u32;
        let f = r.go_fail.find_iter(text).count() as u32;
        if p + f > 0 {
            pass = Some(p);
            fail = Some(f);
        }
    }
    if pass.is_none() && fail.is_none() {
        None
    } else {
        Some(TestCounts {
            pass: pass.unwrap_or(0),
            fail: fail.unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_runners_and_parses_counts_like_oracle() {
        assert_eq!(test_runner("cd app && CI=1 npx vitest run").as_deref(), Some("vitest"));
        assert_eq!(test_runner("cargo test -p station-core").as_deref(), Some("cargo test"));
        assert_eq!(test_runner("npm test --help"), None);
        assert_eq!(test_runner("echo \"npm test\""), None);

        assert_eq!(
            test_counts("test result: ok. 29 passed; 0 failed;"),
            Some(TestCounts { pass: 29, fail: 0 })
        );
        assert_eq!(
            test_counts("# pass 12\n# fail 1\n"),
            Some(TestCounts { pass: 12, fail: 1 })
        );
        assert_eq!(
            test_counts("30 examples, 2 failures"),
            Some(TestCounts { pass: 28, fail: 2 })
        );
    }
}
