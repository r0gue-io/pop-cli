#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod engines;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod style;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod helpers;

#[cfg(feature = "parachain")]
mod git;
#[cfg(feature = "parachain")]
mod parachains;


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
    /// Build a parachain, a pallet or smart contract.
    New(commands::new::NewArgs),
    /// Compile a parachain or smart contract.
    Build(commands::build::BuildArgs),
    /// Deploy a parachain or smart contract.
    Up(commands::up::UpArgs),
    /// Test a smart contract.
    Test(commands::test::TestArgs),
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
        Commands::Build(args) => match &args.command {
            #[cfg(feature = "contract")]
            commands::build::BuildCommands::Contract(cmd) => cmd.execute(),
        },
        Commands::Up(args) => Ok(match &args.command {
            #[cfg(feature = "parachain")]
            commands::up::UpCommands::Parachain(cmd) => cmd.execute().await?,
            #[cfg(feature = "contract")]
            commands::up::UpCommands::Contract(cmd) => cmd.execute().await?,
        }),
        Commands::Test(args) => match &args.command {
            #[cfg(feature = "contract")]
            commands::test::TestCommands::Contract(cmd) => cmd.execute(),
        },
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
