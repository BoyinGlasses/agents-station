use crate::atomic::write_json_atomic;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SERVICE_MS: u64 = 3 * 60 * 1000;
pub const NEW_BUILD_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiftSticker {
    pub build: String,
    pub version: String,
    pub at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiftPersisted {
    #[serde(default)]
    pub build: String,
    #[serde(default)]
    pub sticker: Option<LiftSticker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftPlan {
    pub since: u64,
    pub until: u64,
    #[serde(rename = "newBuild")]
    pub new_build: bool,
    pub sticker: Option<LiftSticker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftView {
    pub left: u64,
    pub total: u64,
    #[serde(rename = "newBuild")]
    pub new_build: bool,
    pub sticker: Option<LiftSticker>,
}

pub fn plan_service(
    last: Option<&LiftPersisted>,
    build: &str,
    now_ms: u64,
    version: &str,
) -> LiftPlan {
    let new_build = !build.is_empty()
        && last.is_some_and(|l| !l.build.is_empty() && l.build != build);
    let until = now_ms.saturating_add(if new_build { NEW_BUILD_MS } else { SERVICE_MS });
    let sticker = if new_build {
        Some(LiftSticker {
            build: build.to_owned(),
            version: version.to_owned(),
            at: until,
        })
    } else if !build.is_empty() {
        last.and_then(|l| l.sticker.as_ref())
            .filter(|s| s.build == build)
            .cloned()
    } else {
        None
    };
    LiftPlan {
        since: now_ms,
        until,
        new_build,
        sticker,
    }
}

pub struct LiftService {
    pub plan: LiftPlan,
}

impl LiftService {
    pub fn init(data_dir: &Path, build: &str, version: &str, now_ms: u64) -> Self {
        let file = data_dir.join("lift.json");
        let last = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<LiftPersisted>(&s).ok());
        let plan = plan_service(last.as_ref(), build, now_ms, version);
        let save_build = if !build.is_empty() {
            build.to_owned()
        } else {
            last.map(|l| l.build).unwrap_or_default()
        };
        let _ = write_json_atomic(
            &file,
            &LiftPersisted {
                build: save_build,
                sticker: plan.sticker.clone(),
            },
        );
        Self { plan }
    }

    pub fn pub_view(&self, now_ms: u64) -> LiftView {
        LiftView {
            left: self.plan.until.saturating_sub(now_ms),
            total: self.plan.until.saturating_sub(self.plan.since),
            new_build: self.plan.new_build,
            sticker: self.plan.sticker.as_ref().map(|s| LiftSticker {
                version: s.version.clone(),
                at: s.at,
                build: s.build.chars().take(7).collect(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lift_service_detects_new_build_and_persists_sticker() {
        let dir = tempfile::tempdir().unwrap();
        let first = LiftService::init(dir.path(), "abcdef1234", "0.1.0", 1000);
        assert!(!first.plan.new_build);
        assert!(first.plan.sticker.is_none());

        let second = LiftService::init(dir.path(), "9999999aaa", "0.2.0", 2000);
        assert!(second.plan.new_build);
        assert_eq!(second.pub_view(2000).total, NEW_BUILD_MS);
        assert_eq!(second.pub_view(2000).sticker.unwrap().build, "9999999");

        let same = LiftService::init(dir.path(), "9999999aaa", "0.2.0", 3000);
        assert!(!same.plan.new_build);
        assert!(same.plan.sticker.is_some());
    }
}
