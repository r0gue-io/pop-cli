// SPDX-License-Identifier: GPL-3.0

use crate::cli::{traits::Cli as _, Cli};
use clap::Args;
use pop_parachains::build_parachain;
use std::{path::PathBuf, thread::sleep, time::Duration};

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
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		let project = if self.package.is_some() { "package" } else { "parachain" };
		Cli.intro(format!("Building your {project}"))?;

		// Show warning if specified as deprecated.
		if !self.valid {
			Cli.warning("NOTE: this command is deprecated. Please use `pop build` (or simply `pop b`) in future...")?;
			sleep(Duration::from_secs(3))
		} else {
			if !self.release {
				Cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
				sleep(Duration::from_secs(3))
			}
		}

		// Build parachain.
		Cli.warning("NOTE: this may take some time...")?;
		build_parachain(self.path.as_deref(), self.package, self.release)?;
		let mode = if self.release { "RELEASE" } else { "DEBUG" };
		Cli.info(format!("The {project} was built in {mode} mode.",))?;
		Cli.outro("Build completed successfully!")?;
		Ok(())
	}
}
