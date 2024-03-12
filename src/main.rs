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

#[cfg(feature = "contract")]
mod signer;

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
	/// Generate a new parachain, pallet or smart contract.
	#[clap(alias = "n")]
	New(commands::new::NewArgs),
	/// Build a parachain or smart contract.
	#[clap(alias = "b")]
	Build(commands::build::BuildArgs),
	/// Call a smart contract.
	#[clap(alias = "c")]
	Call(commands::call::CallArgs),
	/// Deploy a parachain or smart contract.
	#[clap(alias = "u")]
	Up(commands::up::UpArgs),
	/// Test a smart contract.
	#[clap(alias = "t")]
	Test(commands::test::TestArgs),
	#[clap(alias = "a")]
	/// Add a pallet to the runtime
	Add(commands::add::AddArgs),
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
			#[cfg(feature = "parachain")]
			commands::build::BuildCommands::Parachain(cmd) => cmd.execute(),
			#[cfg(feature = "contract")]
			commands::build::BuildCommands::Contract(cmd) => cmd.execute(),
		},
		Commands::Call(args) => Ok(match &args.command {
			#[cfg(feature = "contract")]
			commands::call::CallCommands::Contract(cmd) => cmd.execute().await?,
		}),
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
		Commands::Add(args) => args.execute(),
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
