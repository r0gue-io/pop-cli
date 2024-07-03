// SPDX-License-Identifier: GPL-3.0

use crate::cli::{traits::Cli as _, Cli};
use clap::{Args, Subcommand};
#[cfg(feature = "contract")]
use contract::BuildContractCommand;
use duct::cmd;
#[cfg(feature = "parachain")]
use parachain::BuildParachainCommand;
use std::path::PathBuf;

#[cfg(feature = "contract")]
pub(crate) mod contract;
#[cfg(feature = "parachain")]
pub(crate) mod parachain;

/// Arguments for building a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
	#[command(subcommand)]
	pub command: Option<Command>,
	#[arg(long = "path", help = "Directory path for your project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(long = "release", short)]
	pub(crate) release: bool,
}

/// Build a parachain or smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// [DEPRECATED] Build a parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(BuildParachainCommand),
	/// [DEPRECATED] Build a contract, generate metadata, bundle together in a `<name>.contract` file
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(BuildContractCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<()> {
		// If only contract feature enabled, build as contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(args.path.as_deref())? {
			// All commands originating from root command are valid
			return BuildContractCommand { path: args.path, release: args.release, valid: true }
				.execute();
		}

		// If only parachain feature enabled, build as parachain
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(args.path.as_deref())? {
			// All commands originating from root command are valid
			return BuildParachainCommand { path: args.path, release: args.release, valid: true }
				.execute();
		}

		// Otherwise build as a normal Rust project
		Cli.intro("Building your project")?;

		let mut _args = vec!["build"];
		if args.release {
			_args.push("--release");
		}
		cmd("cargo", _args).dir(args.path.unwrap_or("./".into())).run()?;

		let mode = if args.release { "RELEASE" } else { "DEBUG" };
		Cli.info(format!("The project was built in {mode} mode.",))?;
		Cli.outro("Build completed successfully!")?;
		Ok(())
	}
}
