// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::builds::get_project_path,
};
use clap::{Args, Subcommand};
#[cfg(feature = "contract")]
use contract::BuildContract;
use duct::cmd;
use pop_common::Profile;
use std::path::PathBuf;
#[cfg(feature = "parachain")]
use {parachain::BuildParachain, spec::BuildSpecCommand};

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
	/// Directory path with flag for your project [default: current directory]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The package to be built.
	#[arg(short = 'p', long)]
	pub(crate) package: Option<String>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short, long, conflicts_with = "profile")]
	pub(crate) release: bool,
	/// Build profile [default: debug].
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
}

/// Subcommand for building chain artifacts.
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
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(project_path.as_deref().map(|v| v))? {
			// All commands originating from root command are valid
			let release = match args.profile {
				Some(profile) => profile.into(),
				None => args.release,
			};
			BuildContract { path: project_path, release }.execute()?;
			return Ok("contract");
		}

		// If only parachain feature enabled, build as parachain
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(project_path.as_deref().map(|v| v))? {
			let profile = match args.profile {
				Some(profile) => profile,
				None => args.release.into(),
			};
			let temp_path = PathBuf::from("./");
			BuildParachain {
				path: project_path.unwrap_or_else(|| temp_path).to_path_buf(),
				package: args.package,
				profile,
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
								path_pos: Some(project_path.clone()),
								package: package.clone(),
								release,
								profile: Some(profile.clone()),
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
