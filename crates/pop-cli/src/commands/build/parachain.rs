// SPDX-License-Identifier: GPL-3.0

use crate::cli::{traits::Cli as _, Cli};
use clap::Args;
use pop_parachains::build_parachain;
use std::{path::PathBuf, thread::sleep, time::Duration};

#[derive(Args)]
pub struct BuildParachainCommand {
	#[arg(
		short = 'p',
		long = "path",
		help = "Directory path for your project, [default: current directory]"
	)]
	pub(crate) path: Option<PathBuf>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(long = "release", short, default_value = "true")]
	pub(crate) release: bool,
	// Deprecation flag, used to specify whether the deprecation warning is shown.
	#[clap(skip)]
	pub(crate) valid: bool,
}

impl BuildParachainCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Building your parachain")?;

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
		build_parachain(&self.path)?;
		let mode = if self.release { "RELEASE" } else { "DEBUG" };
		Cli.info(format!("The parachain was built in {mode} mode.",))?;
		Cli.outro("Build Completed Successfully!")?;
		Ok(())
	}
}
