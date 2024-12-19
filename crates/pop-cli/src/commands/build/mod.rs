// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, Cli};
use clap::{Args, Subcommand};
#[cfg(feature = "contract")]
use contract::BuildContractCommand;
use duct::cmd;
use pop_common::Profile;
use std::path::PathBuf;
#[cfg(feature = "parachain")]
use {parachain::BuildParachainCommand, spec::BuildSpecCommand};

#[cfg(feature = "contract")]
pub(crate) mod contract;
#[cfg(feature = "parachain")]
pub(crate) mod parachain;
#[cfg(feature = "parachain")]
pub(crate) mod spec;

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
	#[clap(short, long, conflicts_with = "profile")]
	pub(crate) release: bool,
	/// Build profile [default: debug].
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long = "id")]
	#[cfg(feature = "parachain")]
	pub(crate) id: Option<u32>,
}

/// Build a parachain, smart contract or Rust package.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Build a chain specification and its genesis artifacts.
	#[cfg(feature = "parachain")]
	#[clap(alias = "s")]
	Spec(BuildSpecCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<&'static str> {
		// If only contract feature enabled, build as contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(args.path.as_deref())? {
			// All commands originating from root command are valid
			let release = match args.profile {
				Some(profile) => profile.into(),
				None => args.release,
			};
			BuildContractCommand { path: args.path, release }.execute()?;
			return Ok("contract");
		}

		// If only parachain feature enabled, build as parachain
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(args.path.as_deref())? {
			let profile = match args.profile {
				Some(profile) => profile,
				None => args.release.into(),
			};
			// All commands originating from root command are valid
			BuildParachainCommand {
				path: args.path,
				package: args.package,
				profile: Some(profile),
				id: args.id,
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
		let profile = args.profile.unwrap_or(Profile::Debug);
		if profile == Profile::Release {
			_args.push("--release");
		} else if profile == Profile::Production {
			_args.push("--profile=production");
		}
		cmd("cargo", _args).dir(args.path.unwrap_or_else(|| "./".into())).run()?;

		cli.info(format!("The {project} was built in {} mode.", profile))?;
		cli.outro("Build completed successfully!")?;
		Ok(project)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;
	use pop_common::manifest::add_production_profile;
	use strum::VariantArray;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		add_production_profile(&project_path)?;

		for package in [None, Some(name.to_string())] {
			for release in [true, false] {
				for profile in Profile::VARIANTS {
					let profile = if release { Profile::Release } else { profile.clone() };
					let project = if package.is_some() { "package" } else { "project" };
					let mut cli = MockCli::new()
						.expect_intro(format!("Building your {project}"))
						.expect_info(format!("The {project} was built in {profile} mode."))
						.expect_outro("Build completed successfully!");

					assert_eq!(
						Command::build(
							BuildArgs {
								command: None,
								path: Some(project_path.clone()),
								package: package.clone(),
								release,
								profile: Some(profile.clone()),
								id: None,
							},
							&mut cli,
						)?,
						project
					);
					cli.verify()?;
				}
			}
		}
		Ok(())
	}
}
