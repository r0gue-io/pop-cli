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
	/// Directory path for your project [default: current directory]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// The package to be built.
	#[arg(short = 'p', long)]
	pub(crate) package: Option<String>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short, long)]
	pub(crate) release: bool,
}

/// Build a parachain, smart contract or Rust package.
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
			return BuildParachainCommand {
				path: args.path,
				package: args.package,
				release: args.release,
				valid: true,
			}
			.execute();
		}

		// Otherwise build as a normal Rust project
		let project = if args.package.is_some() { "package" } else { "project" };
		Cli.intro(format!("Building your {project}"))?;

		let mut _args = vec!["build"];
		if let Some(package) = args.package.as_deref() {
			_args.push("--package");
			_args.push(package)
		}
		if args.release {
			_args.push("--release");
		}
		cmd("cargo", _args).dir(args.path.unwrap_or("./".into())).run()?;

		let mode = if args.release { "RELEASE" } else { "DEBUG" };
		Cli.info(format!("The {project} was built in {mode} mode.",))?;
		Cli.outro("Build completed successfully!")?;
		Ok(())
	}
}
