// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{clear_screen, intro, log::warning, outro, set_theme};
use pop_parachains::build_parachain;
use std::path::PathBuf;

#[derive(Args)]
pub struct BuildParachainCommand {
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

impl BuildParachainCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building your parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		warning("NOTE: this may take some time...")?;
		build_parachain(&self.path)?;

		outro("Build Completed Successfully!")?;
		Ok(())
	}
}
