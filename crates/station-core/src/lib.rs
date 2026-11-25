pub mod atomic;
pub mod auth;
pub mod config;
pub mod prefs;
pub mod redact;
pub mod shared;

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
        let checked_or_pending = ledger.lines().filter(|l| l.starts_with("- [")).count();
        assert!(
            checked_or_pending >= 449,
            "expected >= 449 tracked files in PARITY_LEDGER.md, got {checked_or_pending}"
        );
    }
}
