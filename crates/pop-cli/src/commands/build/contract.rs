// SPDX-License-Identifier: GPL-3.0

use crate::style::Theme;
use clap::Args;
use cliclack::{clear_screen, intro, log, outro, set_theme};
use console::style;
use pop_contracts::build_smart_contract;
use std::path::PathBuf;

#[derive(Args)]
pub struct BuildContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
	/// The default compilation includes debug functionality, increasing contract size and gas usage.
	/// For production, always build in release mode to exclude debug features.
	#[clap(long = "release", short)]
	pub(crate) release: bool,
}

impl BuildContractCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building your contract", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		let result_build = build_smart_contract(self.path.as_deref(), self.release)?;
		outro("Build completed successfully!")?;
		log::success(result_build.to_string())?;
		Ok(())
	}
}
