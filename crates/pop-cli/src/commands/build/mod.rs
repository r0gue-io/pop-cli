// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
#[cfg(feature = "contract")]
use contract::BuildContractCommand;
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
	#[arg(
		short = 'p',
		long = "path",
		help = "Directory path for your project, [default: current directory]"
	)]
	pub(crate) path: Option<PathBuf>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(long = "release", short)]
	pub(crate) release: bool,
}

/// Build a parachain or smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Build a parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(BuildParachainCommand),
	/// Build a contract, generate metadata, bundle together in a `<name>.contract` file
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(BuildContractCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<()> {
		// Check if both parachain and contract features enabled
		#[cfg(all(feature = "parachain", feature = "contract"))]
		{
			// Detect if smart contract project, otherwise assume a parachain project
			if pop_contracts::is_smart_contract(args.path.as_deref()) {
				return BuildContractCommand { path: args.path, release: args.release }.execute();
			}
			return BuildParachainCommand { path: args.path, release: args.release }.execute();
		}
		// If only parachain feature enabled, build as parachain
		#[cfg(all(feature = "parachain", not(feature = "contract")))]
		{
			return BuildParachainCommand { path: args.path, release: args.release }.execute();
		}
		// If only contract feature enabled, build as contract
		#[cfg(all(feature = "contract", not(feature = "parachain")))]
		return BuildContractCommand { path: args.path, release: args.release }.execute();
	}
}
