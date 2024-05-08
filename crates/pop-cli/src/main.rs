// SPDX-License-Identifier: GPL-3.0

#[cfg(not(any(feature = "contract", feature = "parachain")))]
compile_error!("feature \"contract\" or feature \"parachain\" must be enabled");

#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
mod style;

#[cfg(feature = "parachain")]
use anyhow::anyhow;
use anyhow::Result;
use clap::{Parser, Subcommand};
#[cfg(feature = "parachain")]
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
	#[cfg(any(feature = "parachain", feature = "contract"))]
	New(commands::new::NewArgs),
	/// Build a parachain or smart contract.
	#[clap(alias = "b")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Build(commands::build::BuildArgs),
	/// Call a smart contract.
	#[clap(alias = "c")]
	#[cfg(feature = "contract")]
	Call(commands::call::CallArgs),
	/// Deploy a parachain or smart contract.
	#[clap(alias = "u")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Up(commands::up::UpArgs),
	/// Test a smart contract.
	#[clap(alias = "t")]
	#[cfg(feature = "contract")]
	Test(commands::test::TestArgs),
	/// Set up the environment for development
	#[clap(alias = "i")]
	Install(commands::install::InstallArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();
	match cli.command {
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::New(args) => match &args.command {
			#[cfg(feature = "parachain")]
			commands::new::NewCommands::Parachain(cmd) => cmd.execute().await,
			#[cfg(feature = "parachain")]
			commands::new::NewCommands::Pallet(cmd) => cmd.execute().await,
			#[cfg(feature = "contract")]
			commands::new::NewCommands::Contract(cmd) => cmd.execute().await,
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::Build(args) => match &args.command {
			#[cfg(feature = "parachain")]
			commands::build::BuildCommands::Parachain(cmd) => cmd.execute(),
			#[cfg(feature = "contract")]
			commands::build::BuildCommands::Contract(cmd) => cmd.execute(),
		},
		#[cfg(feature = "contract")]
		Commands::Call(args) => match &args.command {
			commands::call::CallCommands::Contract(cmd) => cmd.execute().await,
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::Up(args) => match &args.command {
			#[cfg(feature = "parachain")]
			commands::up::UpCommands::Parachain(cmd) => cmd.execute().await,
			#[cfg(feature = "contract")]
			commands::up::UpCommands::Contract(cmd) => cmd.execute().await,
		},
		#[cfg(feature = "contract")]
		Commands::Test(args) => match &args.command {
			commands::test::TestCommands::Contract(cmd) => cmd.execute(),
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::Install(args) => args.execute().await,
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

#[test]
fn verify_cli() {
	// https://docs.rs/clap/latest/clap/_derive/_tutorial/chapter_4/index.html
	use clap::CommandFactory;
	Cli::command().debug_assert()
}
