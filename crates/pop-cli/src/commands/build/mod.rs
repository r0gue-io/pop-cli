// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, Cli};
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
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long = "id")]
	#[cfg(feature = "parachain")]
	pub(crate) id: Option<u32>,
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
	pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<&'static str> {
		// If only contract feature enabled, build as contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(args.path.as_deref())? {
			// All commands originating from root command are valid
			BuildContractCommand { path: args.path, release: args.release, valid: true }
				.execute()?;
			return Ok("contract");
		}

		// If only parachain feature enabled, build as parachain
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(args.path.as_deref())? {
			// All commands originating from root command are valid
			BuildParachainCommand {
				path: args.path,
				package: args.package,
				release: args.release,
				id: args.id,
				valid: true,
			}
			.execute()?;
			return Ok("parachain");
		}

		// Otherwise build as a normal Rust project
		Self::build(args, &mut Cli)
	}

	/// Builds a Rust project.
	///
	/// # Arguments
	/// * `path` - The path to the project.
	/// * `package` - A specific package to be built.
	/// * `release` - Whether the release profile is to be used.
	fn build(args: BuildArgs, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		let project = if args.package.is_some() { "package" } else { "project" };
		cli.intro(format!("Building your {project}"))?;

		let mut _args = vec!["build"];
		if let Some(package) = args.package.as_deref() {
			_args.push("--package");
			_args.push(package)
		}
		if args.release {
			_args.push("--release");
		}
		cmd("cargo", _args).dir(args.path.unwrap_or_else(|| "./".into())).run()?;

		let mode = if args.release { "RELEASE" } else { "DEBUG" };
		cli.info(format!("The {project} was built in {mode} mode."))?;
		cli.outro("Build completed successfully!")?;
		Ok(project)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;

		for package in [None, Some(name.to_string())] {
			for release in [false, true] {
				let project = if package.is_some() { "package" } else { "project" };
				let mode = if release { "RELEASE" } else { "DEBUG" };
				let mut cli = MockCli::new()
					.expect_intro(format!("Building your {project}"))
					.expect_info(format!("The {project} was built in {mode} mode."))
					.expect_outro("Build completed successfully!");

				assert_eq!(
					Command::build(
						BuildArgs {
							command: None,
							path: Some(path.join(name)),
							package: package.clone(),
							release,
							id: None,
						},
						&mut cli,
					)?,
					project
				);

				cli.verify()?;
			}
		}

		Ok(())
	}
}
