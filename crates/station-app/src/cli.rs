use clap::{Args, Parser, Subcommand, ValueEnum};
use station_core::{DEFAULT_HOST, DEFAULT_PORT};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GpuPreference {
    Auto,
    Integrated,
    Discrete,
}

#[derive(Debug, Clone, Args)]
pub struct HookCliArgs {
    pub url: String,
    #[arg(long = "async")]
    pub r#async: bool,
    #[arg(long)]
    pub fast: bool,
    #[arg(long)]
    pub wait: Option<u64>,
    #[arg(long)]
    pub job: bool,
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long)]
    pub event: Option<String>,
    #[arg(long)]
    pub response_json: bool,
}

#[derive(Debug, Parser)]
#[command(name = "agents-station", version, about = "Agents Station native desktop app")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, env = "OFFICE_PORT", default_value_t = DEFAULT_PORT)]
    pub port: u16,

    #[arg(long, env = "OFFICE_HOST", default_value = DEFAULT_HOST)]
    pub host: String,

    #[arg(long, value_enum, default_value_t = GpuPreference::Auto)]
    pub gpu: GpuPreference,

    #[arg(long)]
    pub capture_scene: Option<String>,

    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Gui,
    Serve {
        #[arg(long)]
        lan: bool,
    },
    Hook(HookCliArgs),
    Install {
        #[arg(long, default_value = "claude")]
        agent: String,
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    Uninstall {
        #[arg(long, default_value = "claude")]
        agent: String,
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    Service {
        action: String,
        #[arg(long)]
        lan: bool,
    },
    Doctor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_defaults_match_spec_and_reject_invalid_gpu() {
        let parsed = Cli::try_parse_from(["agents-station"]).unwrap();
        assert_eq!(parsed.port, 4310);
        assert_eq!(parsed.host, "127.0.0.1");
        assert_eq!(parsed.gpu, GpuPreference::Auto);
        assert!(Cli::try_parse_from(["agents-station", "--gpu", "bogus"]).is_err());

        let cap = Cli::try_parse_from([
            "agents-station",
            "--capture-scene",
            "smoke",
            "--out",
            "out.png",
            "--gpu",
            "integrated",
        ])
        .unwrap();
        assert_eq!(cap.capture_scene.as_deref(), Some("smoke"));
        assert_eq!(cap.out.as_deref(), Some(std::path::Path::new("out.png")));
        assert_eq!(cap.gpu, GpuPreference::Integrated);
    }

    #[test]
    fn cli_parses_hook_and_serve_subcommands() {
        let hook = Cli::try_parse_from([
            "agents-station",
            "hook",
            "http://127.0.0.1:4310/event",
            "--async",
            "--agent",
            "codex",
            "--event",
            "Stop",
            "--response-json",
        ])
        .unwrap();
        match hook.command {
            Some(Command::Hook(args)) => {
                assert_eq!(args.url, "http://127.0.0.1:4310/event");
                assert!(args.r#async);
                assert_eq!(args.agent.as_deref(), Some("codex"));
                assert_eq!(args.event.as_deref(), Some("Stop"));
                assert!(args.response_json);
            }
            other => panic!("expected Hook command, got {other:?}"),
        }

        let serve = Cli::try_parse_from(["agents-station", "serve", "--lan"]).unwrap();
        assert!(matches!(serve.command, Some(Command::Serve { lan: true })));
    }
}
