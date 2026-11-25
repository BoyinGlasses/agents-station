pub mod cli;
pub mod instance;

use clap::Parser;

const GUI_MUTEX_NAME: &str = "Local\\agents-station-gui";

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let is_gui = matches!(cli.command, None | Some(cli::Command::Gui))
        && cli.capture_scene.is_none();

    let _lock = if is_gui {
        match instance::InstanceLock::try_acquire(GUI_MUTEX_NAME)? {
            Some(lock) => Some(lock),
            None => return Ok(()),
        }
    } else {
        None
    };

    println!(
        "agents-station bootstrap ({}:{}, gpu={:?})",
        cli.host, cli.port, cli.gpu
    );
    Ok(())
}
