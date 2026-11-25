use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoSession {
    pub id: String,
    pub agent: String,
    pub name: String,
    pub status: String, // "work" | "wait" | "rest"
    pub last_prompt: String,
    pub tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoLogEntry {
    pub ts_offset_ms: u64,
    pub agent: String,
    pub text: String,
}

pub fn build_demo_sessions() -> Vec<DemoSession> {
    vec![
        DemoSession {
            id: "demo-claude".into(),
            agent: "claude".into(),
            name: "Claude".into(),
            status: "work".into(),
            last_prompt: "Refactor station-3d shared mesh palette".into(),
            tool: "Edit".into(),
        },
        DemoSession {
            id: "demo-codex".into(),
            agent: "codex".into(),
            name: "Codex".into(),
            status: "work".into(),
            last_prompt: "Run contract tests & soak gate".into(),
            tool: "Bash".into(),
        },
        DemoSession {
            id: "demo-opencode".into(),
            agent: "opencode".into(),
            name: "OpenCode".into(),
            status: "work".into(),
            last_prompt: "Audit Vietnamese i18n keys".into(),
            tool: "Read".into(),
        },
        DemoSession {
            id: "demo-zcode".into(),
            agent: "zcode".into(),
            name: "ZCode".into(),
            status: "wait".into(),
            last_prompt: "Approve release bundle publish?".into(),
            tool: "AskUserQuestion".into(),
        },
        DemoSession {
            id: "demo-antigravity".into(),
            agent: "antigravity".into(),
            name: "Antigravity".into(),
            status: "work".into(),
            last_prompt: "Verify side-by-side PNG visual parity".into(),
            tool: "WebFetch".into(),
        },
    ]
}

pub fn build_demo_log() -> Vec<DemoLogEntry> {
    vec![
        DemoLogEntry { ts_offset_ms: 0, agent: "claude".into(), text: "Edited crates/station-3d/src/palette.rs".into() },
        DemoLogEntry { ts_offset_ms: 12_000, agent: "codex".into(), text: "cargo test --workspace: all green".into() },
        DemoLogEntry { ts_offset_ms: 28_000, agent: "opencode".into(), text: "Checked 105 i18n locale files".into() },
        DemoLogEntry { ts_offset_ms: 45_000, agent: "zcode".into(), text: "Waiting for permission approval".into() },
        DemoLogEntry { ts_offset_ms: 61_000, agent: "antigravity".into(), text: "Captured Cozy, Pro & HQ screenshots".into() },
    ]
}
