// SPDX-License-Identifier: GPL-3.0

use crate::cli::{traits::Cli as _, Cli};
use clap::Args;
use pop_contracts::build_smart_contract;
use std::{path::PathBuf, thread::sleep, time::Duration};

#[derive(Args)]
pub struct BuildContractCommand {
	#[arg(long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
	/// The default compilation includes debug functionality, increasing contract size and gas usage.
	/// For production, always build in release mode to exclude debug features.
	#[clap(long = "release", short)]
	pub(crate) release: bool,
	// Deprecation flag, used to specify whether the deprecation warning is shown.
	#[clap(skip)]
	pub(crate) valid: bool,
}

impl BuildContractCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Building your contract")?;

		// Show warning if specified as deprecated.
		if !self.valid {
			Cli.warning("NOTE: this command is deprecated. Please use `pop build` (or simply `pop b`) in future...")?;
			sleep(Duration::from_secs(3));
		}

		// Build contract.
		let build_result = build_smart_contract(self.path.as_deref(), self.release)?;
		Cli.success(build_result)?;
		Cli.outro("Build completed successfully!")?;
		Ok(())
	}
}
