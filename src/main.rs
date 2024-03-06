#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod engines;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod style;

#[cfg(feature = "parachain")]
mod git;
#[cfg(feature = "parachain")]
mod helpers;
#[cfg(feature = "parachain")]
mod parachains;


use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use commands::{build, new, add, up, test};
use std::{fs::create_dir_all, path::PathBuf};

#[derive(Parser)]
#[command(author, version, about, styles=style::get_styles())]
pub struct Cli {
    #[command(subcommand)]
    branch: Branch,
}

#[derive(Subcommand)]
#[command(subcommand_required = true)]
enum Branch {
    /// Build a parachain, a pallet or smart contract.
    /// Create a new parachain or smart contract.
    New(new::NewArgs),
    /// Compile a parachain or smart contract.
    Build(build::BuildArgs),
    /// Deploy a parachain or smart contract.
    Up(up::UpArgs),
    /// Add a pallet into a runtime
    Add(add::AddArgs),
    /// Test a smart contract.
    Test(test::TestArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.branch {
        Branch::New(args) => match &args.command {
            #[cfg(feature = "parachain")]
            new::NewCommands::Parachain(cmd) => cmd.execute(),
            #[cfg(feature = "parachain")]
            new::NewCommands::Pallet(cmd) => cmd.execute(),
            #[cfg(feature = "contract")]
            new::NewCommands::Contract(cmd) => cmd.execute(),
        },
        Branch::Build(args) => match &args.command {
            #[cfg(feature = "contract")]
            build::BuildCommands::Contract(cmd) => cmd.execute(),
        },
        Branch::Up(args) => Ok(match &args.command {
            #[cfg(feature = "parachain")]
            up::UpCommands::Parachain(cmd) => cmd.execute().await?,
        }),
        Branch::Test(args) => match &args.command {
            #[cfg(feature = "contract")]
            test::TestCommands::Contract(cmd) => cmd.execute(),
        },
        Branch::Add(args) => args.execute(),
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
