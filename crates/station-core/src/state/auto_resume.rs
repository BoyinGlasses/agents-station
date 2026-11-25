use serde::Serialize;
use std::collections::HashMap;

fn valid_uuid_sid(sid: &str) -> bool {
    sid.len() == 36
        && sid
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
}

#[derive(Debug, Clone)]
struct AttemptRecord {
    count: u32,
    first_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResumeDecision {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
}

pub struct AutoResumeGuard {
    max_attempts: u32,
    window_ms: u64,
    history: HashMap<String, AttemptRecord>,
}

impl Default for AutoResumeGuard {
    fn default() -> Self {
        Self::new(3, 10 * 60 * 1000)
    }
}

impl AutoResumeGuard {
    pub fn new(max_attempts: u32, window_ms: u64) -> Self {
        Self {
            max_attempts,
            window_ms,
            history: HashMap::new(),
        }
    }

    pub fn check_and_record(
        &mut self,
        enabled: bool,
        sid: &str,
        has_claude_bin: bool,
        now_ms: u64,
    ) -> ResumeDecision {
        if !enabled {
            return ResumeDecision {
                ok: false,
                reason: Some("disabled"),
                attempts: None,
            };
        }
        if !valid_uuid_sid(sid) {
            return ResumeDecision {
                ok: false,
                reason: Some("badSid"),
                attempts: None,
            };
        }
        let rec = self
            .history
            .entry(sid.to_owned())
            .or_insert(AttemptRecord {
                count: 0,
                first_at: now_ms,
            });
        if now_ms.saturating_sub(rec.first_at) > self.window_ms {
            rec.count = 0;
            rec.first_at = now_ms;
        }
        rec.count += 1;
        if rec.count > self.max_attempts {
            return ResumeDecision {
                ok: false,
                reason: Some("rateLimited"),
                attempts: Some(rec.count),
            };
        }
        if !has_claude_bin {
            return ResumeDecision {
                ok: false,
                reason: Some("noClaude"),
                attempts: None,
            };
        }
        ResumeDecision {
            ok: true,
            reason: None,
            attempts: Some(rec.count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_resume_enforces_uuid_enabled_and_rate_limit() {
        let mut g = AutoResumeGuard::default();
        let sid = "12345678-1234-1234-1234-1234567890ab";
        assert_eq!(g.check_and_record(false, sid, true, 1000).reason, Some("disabled"));
        assert_eq!(g.check_and_record(true, "bad", true, 1000).reason, Some("badSid"));
        for _ in 0..3 {
            assert!(g.check_and_record(true, sid, true, 2000).ok);
        }
        assert_eq!(g.check_and_record(true, sid, true, 3000).reason, Some("rateLimited"));
        // After window resets
        assert!(g.check_and_record(true, sid, true, 2000 + 11 * 60 * 1000).ok);
    }
}
