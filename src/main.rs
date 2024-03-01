mod commands;
#[cfg(feature = "parachain")]
mod engines;
#[cfg(feature = "parachain")]
mod git;
#[cfg(feature = "parachain")]
mod helpers;
#[cfg(feature = "parachain")]
mod parachains;
mod style;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::{fs::create_dir_all, path::PathBuf};

#[derive(Parser)]
#[command(author, version, about, styles=style::get_styles())]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[command(subcommand_required = true)]
enum Commands {
    New(commands::new::NewArgs),
    /// Deploy a parachain or smart contract.
    Up(commands::up::UpArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::New(args) => match &args.command {
            #[cfg(feature = "parachain")]
            commands::new::NewCommands::Parachain(cmd) => cmd.execute(),
            #[cfg(feature = "parachain")]
            commands::new::NewCommands::Pallet(cmd) => cmd.execute(),
            #[cfg(feature = "contract")]
            commands::new::NewCommands::Contract(cmd) => cmd.execute(),
        },
        Commands::Up(args) => Ok(match &args.command {
            #[cfg(feature = "parachain")]
            commands::up::UpCommands::Parachain(cmd) => cmd.execute().await?,
        }),
    }
}

#[cfg(feature = "parachain")]
fn cache() -> Result<PathBuf> {
    let cache_path = dirs::cache_dir()
        .ok_or(anyhow!("the cache directory could not be determined"))?
        .join("pop");
    // Creates pop dir if needed
    create_dir_all(cache_path.as_path())?;
    Ok(cache_path)
}
