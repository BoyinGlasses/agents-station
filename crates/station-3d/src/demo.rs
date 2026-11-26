//! Live `Demo` simulation emitter (`demo.js` + `demo-log.js`):
//! Runs the 6 fictional Oracle projects (`shop-api`, `landing-page`, `data-pipeline`,
//! `mobile-app`, `docs-site`, `infra`) with multi-step tool timelines, subagents, tests,
//! commits/pushes/releases, deploys, context compaction, a scripted 45s incident burst,
//! bounded per-session/subagent logs (`DEMO_LOG_MAX=120`, `DEMO_SUB_LOGS_MAX=30`), and
//! the 4-hour battery discharge/recharge cycle.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

pub const DEMO_LOG_MAX: usize = 120;
pub const DEMO_SUB_LOGS_MAX: usize = 30;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoSubagent {
    pub id: String,
    pub kind: String,
    pub desc: String,
    pub steps: u32,
}

#[derive(Debug, Clone)]
pub struct LiveDemoSession {
    pub id: String,
    pub agent: String,
    pub name: String,
    pub dir: String,
    pub desk: usize,
    pub status: String,
    pub last_prompt: String,
    pub tool_kind: Option<String>,
    pub tool_label: Option<String>,
    pub cost_usd: f64,
    pub ctx_tokens: u64,
    pub ctx_max: u64,
    pub commits: u32,
    pub subagents: Vec<DemoSubagent>,
    pub task_idx: usize,
    pub step_idx: usize,
    pub next_step_at: f32,
    pub resting: bool,
    pub waiting_perm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoEmittedEvent {
    pub seq: u64,
    pub session_id: String,
    pub project: String,
    pub agent: String,
    pub desk: usize,
    pub event_type: String,
    pub label: String,
    pub subagent_id: Option<String>,
}

struct DemoProjectSpec {
    name: &'static str,
    dir: &'static str,
    agent: &'static str,
    prompt: &'static str,
    steps: &'static [(&'static str, &'static str)],
    deploy: Option<&'static str>,
    has_sub: Option<(&'static str, &'static str)>,
    has_wait: Option<&'static str>,
}

const DEMO_PROJECTS: [DemoProjectSpec; 6] = [
    DemoProjectSpec {
        name: "shop-api",
        dir: "~/projects/shop-api",
        agent: "claude",
        prompt: "Fix cart pagination edge cases and add tests",
        steps: &[
            ("read", "src/routes/cart.ts"),
            ("grep", "paginate"),
            ("edit", "src/routes/cart.ts"),
            ("test", "npm test"),
            ("testfail", "npm test"),
            ("edit", "src/routes/cart.test.ts"),
            ("test", "npm test"),
            ("testok", "npm test"),
        ],
        deploy: Some("v2.4.1"),
        has_sub: None,
        has_wait: Some("Approve DB migration for cart index?"),
    },
    DemoProjectSpec {
        name: "landing-page",
        dir: "~/projects/landing-page",
        agent: "codex",
        prompt: "Refresh hero visual and optimize WebP images",
        steps: &[
            ("read", "app/page.tsx"),
            ("edit", "components/Hero.tsx"),
            ("write", "components/HeroArt.tsx"),
            ("bash", "npm run build"),
            ("fetch", "dribbble.com"),
        ],
        deploy: None,
        has_sub: Some(("general-purpose", "Audit hero contrast & responsive layout")),
        has_wait: None,
    },
    DemoProjectSpec {
        name: "data-pipeline",
        dir: "~/projects/data-pipeline",
        agent: "opencode",
        prompt: "Deduplicate orders ETL and verify daily DAG",
        steps: &[
            ("read", "etl/orders.py"),
            ("edit", "etl/orders.py"),
            ("test", "pytest"),
            ("testok", "pytest"),
            ("web", "pandas drop_duplicates subset"),
        ],
        deploy: None,
        has_sub: Some(("Explore", "Inspect Airflow daily schedule DAG")),
        has_wait: Some("Approve backfill on staging warehouse?"),
    },
    DemoProjectSpec {
        name: "mobile-app",
        dir: "~/projects/mobile-app",
        agent: "zcode",
        prompt: "Fix Google OAuth login & keyboard avoidance",
        steps: &[
            ("read", "src/screens/Login.tsx"),
            ("web", "expo auth session google"),
            ("edit", "src/screens/Login.tsx"),
            ("bash", "tsc --noEmit"),
            ("test", "playwright"),
            ("testok", "playwright"),
        ],
        deploy: None,
        has_sub: None,
        has_wait: None,
    },
    DemoProjectSpec {
        name: "docs-site",
        dir: "~/projects/docs-site",
        agent: "antigravity",
        prompt: "Update installation guide and build static docs",
        steps: &[
            ("read", "docs/intro.md"),
            ("write", "docs/install.md"),
            ("bash", "npm run docs:build"),
        ],
        deploy: Some("docs"),
        has_sub: None,
        has_wait: None,
    },
    DemoProjectSpec {
        name: "infra",
        dir: "~/projects/infra",
        agent: "claude",
        prompt: "Update staging Terraform autoscaling rules",
        steps: &[
            ("read", "terraform/staging.tf"),
            ("edit", "terraform/staging.tf"),
            ("bash", "terraform plan"),
        ],
        deploy: None,
        has_sub: None,
        has_wait: Some("Run terraform apply on staging?"),
    },
];

#[derive(Resource, Debug, Clone, Default)]
pub struct DemoSimState {
    pub active: bool,
    pub sim_secs: f32,
    pub seq: u64,
    pub release_minor: u32,
    pub incident_fired: bool,
    pub sessions: Vec<LiveDemoSession>,
    pub logs: HashMap<String, VecDeque<String>>,
    pub recent_events: Vec<DemoEmittedEvent>,
    sub_order: VecDeque<String>,
}

impl DemoSimState {
    pub fn start(&mut self) {
        self.active = true;
        self.sim_secs = 0.0;
        self.seq = 0;
        self.release_minor = 3;
        self.incident_fired = false;
        self.logs.clear();
        self.sub_order.clear();
        self.recent_events.clear();
        self.sessions = DEMO_PROJECTS
            .iter()
            .enumerate()
            .map(|(i, p)| LiveDemoSession {
                id: format!("demo-{i}"),
                agent: p.agent.to_owned(),
                name: p.name.to_owned(),
                dir: p.dir.to_owned(),
                desk: i,
                status: "rest".to_owned(),
                last_prompt: p.prompt.to_owned(),
                tool_kind: None,
                tool_label: None,
                cost_usd: 0.4 + i as f64 * 0.9,
                ctx_tokens: if i == 0 { 168_000 } else { 38_000 + i as u64 * 21_000 },
                ctx_max: 200_000,
                commits: 0,
                subagents: Vec::new(),
                task_idx: 0,
                step_idx: 0,
                next_step_at: 0.4 + i as f32 * 0.65,
                resting: true,
                waiting_perm: false,
            })
            .collect();
        self.push_log("demo-0", "Continued from previous cart pagination session".to_owned());
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.sessions.clear();
        self.logs.clear();
        self.sub_order.clear();
        self.recent_events.clear();
    }

    /// Bounded conversation & subagent log store matching `demo-log.js` (`LOG_MAX=120`, `SUB_LOGS=30`).
    pub fn push_log(&mut self, key: &str, line: String) {
        if key.starts_with("demo-sub-") && !self.logs.contains_key(key) {
            self.sub_order.push_back(key.to_owned());
            while self.sub_order.len() > DEMO_SUB_LOGS_MAX {
                if let Some(old) = self.sub_order.pop_front() {
                    self.logs.remove(&old);
                }
            }
        }
        let entry = self.logs.entry(key.to_owned()).or_default();
        entry.push_back(line);
        while entry.len() > DEMO_LOG_MAX {
            entry.pop_front();
        }
    }

    /// Advances the live demo timeline by `dt_secs` and returns all newly emitted events.
    pub fn tick(&mut self, dt_secs: f32) -> Vec<DemoEmittedEvent> {
        if !self.active || self.sessions.is_empty() {
            return Vec::new();
        }
        let dt = dt_secs.clamp(0.0, 0.5);
        self.sim_secs += dt;
        let now = self.sim_secs;
        let mut emitted = Vec::new();

        // Scripted 45s incident burst (`demo.js:218`): 3 consecutive failures
        if !self.incident_fired && now >= 45.0 {
            self.incident_fired = true;
            for k in 0..3 {
                let idx = k % self.sessions.len();
                self.seq += 1;
                let s = &self.sessions[idx];
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "fail".to_owned(),
                    label: "CI smoke suite failed on staging".to_owned(),
                    subagent_id: None,
                });
            }
        }

        let mut pending_logs: Vec<(String, String)> = Vec::new();

        for i in 0..self.sessions.len() {
            if now < self.sessions[i].next_step_at {
                continue;
            }
            let spec = &DEMO_PROJECTS[i % DEMO_PROJECTS.len()];
            let s = &mut self.sessions[i];

            // Start a new task from rest
            if s.resting {
                s.resting = false;
                s.waiting_perm = false;
                s.status = "work".to_owned();
                s.step_idx = 0;
                s.last_prompt = spec.prompt.to_owned();
                s.tool_kind = Some("prompt".to_owned());
                s.tool_label = Some(spec.prompt.to_owned());
                s.next_step_at = now + 1.8;
                self.seq += 1;
                pending_logs.push((s.id.clone(), format!("User: {}", spec.prompt)));
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "prompt".to_owned(),
                    label: spec.prompt.to_owned(),
                    subagent_id: None,
                });
                continue;
            }

            // Resume from wait state
            if s.waiting_perm {
                s.waiting_perm = false;
                s.status = "work".to_owned();
                s.tool_kind = Some("bash".to_owned());
                s.tool_label = Some(spec.has_wait.unwrap_or("Approved").to_owned());
                s.next_step_at = now + 1.8;
                self.seq += 1;
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "bash".to_owned(),
                    label: spec.has_wait.unwrap_or("Approved").to_owned(),
                    subagent_id: None,
                });
                continue;
            }

            // Emit next scripted step
            if s.step_idx < spec.steps.len() {
                let (ev_type, label) = spec.steps[s.step_idx];
                let mid = spec.steps.len() / 2;

                // Spawn subagent on step 1 if configured
                if s.step_idx == 1 {
                    if let Some((kind, desc)) = spec.has_sub {
                        self.seq += 1;
                        let sub_id = format!("demo-sub-{}", self.seq);
                        s.subagents.push(DemoSubagent {
                            id: sub_id.clone(),
                            kind: kind.to_owned(),
                            desc: desc.to_owned(),
                            steps: 1,
                        });
                        pending_logs.push((sub_id.clone(), format!("{kind}: {desc}")));
                        emitted.push(DemoEmittedEvent {
                            seq: self.seq,
                            session_id: s.id.clone(),
                            project: s.name.clone(),
                            agent: s.agent.clone(),
                            desk: s.desk,
                            event_type: "substart".to_owned(),
                            label: kind.to_owned(),
                            subagent_id: Some(sub_id),
                        });
                    }
                }

                // Pause for permission midway if configured
                if s.step_idx == mid {
                    if let Some(wait_txt) = spec.has_wait {
                        s.step_idx += 1;
                        s.waiting_perm = true;
                        s.status = "wait".to_owned();
                        s.tool_kind = Some("wait".to_owned());
                        s.tool_label = Some(wait_txt.to_owned());
                        s.next_step_at = now + 4.5;
                        self.seq += 1;
                        emitted.push(DemoEmittedEvent {
                            seq: self.seq,
                            session_id: s.id.clone(),
                            project: s.name.clone(),
                            agent: s.agent.clone(),
                            desk: s.desk,
                            event_type: "wait".to_owned(),
                            label: wait_txt.to_owned(),
                            subagent_id: None,
                        });
                        continue;
                    }
                }

                s.step_idx += 1;
                s.tool_kind = Some(ev_type.to_owned());
                s.tool_label = Some(label.to_owned());
                s.ctx_tokens = (s.ctx_tokens + 2_200).min(190_000);
                s.cost_usd += 0.03;
                s.next_step_at = now + if ev_type == "test" { 2.8 } else { 2.0 };
                self.seq += 1;
                pending_logs.push((s.id.clone(), format!("{ev_type}: {label}")));
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: ev_type.to_owned(),
                    label: label.to_owned(),
                    subagent_id: None,
                });
                continue;
            }

            // End of task: close subagents, commit, push every 3rd, release every 6th, deploy, compact, stop
            for sub in s.subagents.drain(..) {
                self.seq += 1;
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "substop".to_owned(),
                    label: sub.kind,
                    subagent_id: Some(sub.id),
                });
            }

            s.commits += 1;
            self.seq += 1;
            emitted.push(DemoEmittedEvent {
                seq: self.seq,
                session_id: s.id.clone(),
                project: s.name.clone(),
                agent: s.agent.clone(),
                desk: s.desk,
                event_type: "commit".to_owned(),
                label: spec.prompt.to_owned(),
                subagent_id: None,
            });

            if s.commits % 3 == 0 {
                self.seq += 1;
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "push".to_owned(),
                    label: "main".to_owned(),
                    subagent_id: None,
                });
            }
            if s.commits % 6 == 0 {
                self.release_minor += 1;
                self.seq += 1;
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "release".to_owned(),
                    label: format!("v1.{}.0", self.release_minor),
                    subagent_id: None,
                });
            }
            if let Some(dep) = spec.deploy {
                self.seq += 1;
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "deploy".to_owned(),
                    label: dep.to_owned(),
                    subagent_id: None,
                });
            }
            if s.ctx_tokens > 150_000 {
                s.ctx_tokens = 42_000;
                self.seq += 1;
                emitted.push(DemoEmittedEvent {
                    seq: self.seq,
                    session_id: s.id.clone(),
                    project: s.name.clone(),
                    agent: s.agent.clone(),
                    desk: s.desk,
                    event_type: "compact".to_owned(),
                    label: "auto".to_owned(),
                    subagent_id: None,
                });
            }

            s.resting = true;
            s.status = "rest".to_owned();
            s.task_idx += 1;
            s.next_step_at = now + 6.0;
            self.seq += 1;
            emitted.push(DemoEmittedEvent {
                seq: self.seq,
                session_id: s.id.clone(),
                project: s.name.clone(),
                agent: s.agent.clone(),
                desk: s.desk,
                event_type: "stop".to_owned(),
                label: "Turn finished".to_owned(),
                subagent_id: None,
            });
        }

        for (k, line) in pending_logs {
            self.push_log(&k, line);
        }

        if !emitted.is_empty() {
            self.recent_events.extend(emitted.iter().cloned());
            if self.recent_events.len() > DEMO_LOG_MAX {
                let drain = self.recent_events.len() - DEMO_LOG_MAX;
                self.recent_events.drain(..drain);
            }
        }
        emitted
    }
}

/// Fictional 4-hour demo battery cycle (`demo.js:123-126`): discharges 100% -> 12% over 200 min,
/// then charges 12% -> 100% over 40 min.
pub fn demo_battery(office_minutes: f32) -> (u8, bool) {
    let m = office_minutes.rem_euclid(240.0);
    if m < 200.0 {
        let pct = (100.0 - m * 0.44).round().clamp(12.0, 100.0) as u8;
        (pct, false)
    } else {
        let pct = (12.0 + (m - 200.0) * 2.2).round().clamp(12.0, 100.0) as u8;
        (pct, true)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_sim_emitter_ticks_six_projects_emits_workflow_events_and_bounds_logs() {
        let mut demo = DemoSimState::default();
        demo.start();
        assert!(demo.active);
        assert_eq!(demo.sessions.len(), 6, "matches 6 Oracle demo projects");

        let mut all_events = Vec::new();
        for _ in 0..240 {
            all_events.extend(demo.tick(0.25));
        }
        assert!(all_events.len() >= 15, "expected live demo timeline events, got {}", all_events.len());
        assert!(all_events.iter().any(|e| e.event_type == "prompt"));
        assert!(all_events.iter().any(|e| e.event_type == "test" || e.event_type == "testok" || e.event_type == "testfail"));
        assert!(all_events.iter().any(|e| e.event_type == "fail"), "expected scripted 45s incident burst");

        // Bounded log ring check (`demo-log.js`: LOG_MAX = 120, SUB_LOGS = 30)
        for i in 0..200 {
            demo.push_log("demo-0", format!("line {i}"));
        }
        assert!(demo.logs.get("demo-0").unwrap().len() <= DEMO_LOG_MAX);
        for i in 0..45 {
            demo.push_log(&format!("demo-sub-{i}"), "step".into());
        }
        let sub_keys = demo.logs.keys().filter(|k| k.starts_with("demo-sub-")).count();
        assert!(sub_keys <= DEMO_SUB_LOGS_MAX);

        // Battery discharge & recharge curve (`demo.js:123`)
        let (b_start, chg_start) = demo_battery(0.0);
        let (b_low, chg_low) = demo_battery(195.0);
        let (b_chg, chg_on) = demo_battery(215.0);
        assert_eq!(b_start, 100);
        assert!(!chg_start);
        assert!(b_low < 20 && !chg_low);
        assert!(chg_on && b_chg > b_low);

        demo.stop();
        assert!(!demo.active);
        assert!(demo.sessions.is_empty());
    }
}
