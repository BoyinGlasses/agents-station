pub mod agent_events;
pub mod atomic;
pub mod auth;
pub mod config;
pub mod hook_client;
pub mod hooks_cli;
pub mod http;
pub mod platform;
pub mod prefs;
pub mod redact;
pub mod shared;
pub mod state;
pub mod store;
pub mod system;

pub const DEFAULT_PORT: u16 = 4310;
pub const DEFAULT_HOST: &str = "127.0.0.1";

pub const WORKSPACE_CRATES: [&str; 6] = [
    "station-core",
    "station-launcher",
    "station-canvas2d",
    "station-3d",
    "station-ui",
    "station-app",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn default_network_constants_match_spec() {
        assert_eq!(DEFAULT_PORT, 4310);
        assert_eq!(DEFAULT_HOST, "127.0.0.1");
        assert_eq!(WORKSPACE_CRATES.len(), 6);
    }

    #[test]
    fn parity_ledger_tracks_every_source_file_without_omissions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ledger = std::fs::read_to_string(root.join("PARITY_LEDGER.md"))
            .expect("PARITY_LEDGER.md must exist at workspace root");

        let mut verified = 0usize;
        let mut partial = 0usize;
        let mut unported = 0usize;

        for (idx, line) in ledger.lines().enumerate() {
            if !line.starts_with("- [") {
                continue;
            }
            if line.starts_with("- [x] ") {
                for forbidden in [
                    "post_local_ok",
                    "get_generic_ok",
                    "post_accounts_any",
                    "Platform trait stub",
                    "draw_qr_block",
                ] {
                    assert!(
                        !line.contains(forbidden),
                        "PARITY_LEDGER.md line {} falsely marks `{forbidden}` as [x]: {line}",
                        idx + 1
                    );
                }
                verified += 1;
            } else if line.starts_with("- [~] ") {
                partial += 1;
            } else if line.starts_with("- [ ] ") {
                unported += 1;
            } else {
                panic!(
                    "malformed parity checkbox on line {}: {line}",
                    idx + 1
                );
            }
        }

        let total = verified + partial + unported;
        assert!(
            total >= 571,
            "expected >= 571 tracked items in PARITY_LEDGER.md, got {total}"
        );

        let expected_status = format!(
            "Status: {verified} verified `[x]` | {partial} partial/stubbed `[~]` | {unported} unported `[ ]` | {total} total"
        );
        assert!(
            ledger.lines().any(|l| l.trim() == expected_status),
            "PARITY_LEDGER.md missing exact summary line `{expected_status}`"
        );
    }
}
