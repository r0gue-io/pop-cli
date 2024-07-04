// SPDX-License-Identifier: GPL-3.0

use crate::cli;
use clap::Args;
use pop_parachains::build_parachain;
use std::path::PathBuf;
#[cfg(not(test))]
use std::{thread::sleep, time::Duration};

#[derive(Args)]
pub struct BuildParachainCommand {
	/// Directory path for your project [default: current directory]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// The package to be built.
	#[arg(short = 'p', long)]
	pub(crate) package: Option<String>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short, long, default_value = "true")]
	pub(crate) release: bool,
	// Deprecation flag, used to specify whether the deprecation warning is shown.
	#[clap(skip)]
	pub(crate) valid: bool,
}

impl BuildParachainCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		let project = if self.package.is_some() { "package" } else { "parachain" };
		cli.intro(format!("Building your {project}"))?;

		// Show warning if specified as deprecated.
		if !self.valid {
			cli.warning("NOTE: this command is deprecated. Please use `pop build` (or simply `pop b`) in future...")?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3))
		} else {
			if !self.release {
				cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
				#[cfg(not(test))]
				sleep(Duration::from_secs(3))
			}
		}

		// Build parachain.
		cli.warning("NOTE: this may take some time...")?;
		build_parachain(self.path.as_deref(), self.package, self.release)?;
		let mode = if self.release { "RELEASE" } else { "DEBUG" };
		cli.info(format!("The {project} was built in {mode} mode.",))?;
		cli.outro("Build completed successfully!")?;
		Ok(project)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;
	use duct::cmd;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";

		for package in [None, Some(name.to_string())] {
			for release in [false, true] {
				for valid in [false, true] {
					let temp_dir = tempfile::tempdir()?;
					let path = temp_dir.path();
					cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
					let project = if package.is_some() { "package" } else { "parachain" };
					let mode = if release { "RELEASE" } else { "DEBUG" };
					let mut cli = MockCli::new()
						.expect_intro(format!("Building your {project}"))
						.expect_warning("NOTE: this may take some time...")
						.expect_info(format!("The {project} was built in {mode} mode."))
						.expect_outro("Build completed successfully!");

					if !valid {
						cli = cli.expect_warning("NOTE: this command is deprecated. Please use `pop build` (or simply `pop b`) in future...");
					} else {
						if !release {
							cli = cli.expect_warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...");
						}
					}

					assert_eq!(
						BuildParachainCommand {
							path: Some(path.join(name)),
							package: package.clone(),
							release,
							valid,
						}
						.build(&mut cli)?,
						project
					);

					cli.verify()?;
				}
			}
		}

		Ok(())
	}
}
