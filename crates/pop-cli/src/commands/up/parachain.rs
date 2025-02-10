// SPDX-License-Identifier: GPL-3.0

use crate::cli;
use clap::Args;
use std::path::PathBuf;

const HELP_HEADER: &str = "Parachain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpParachainCommand {
	/// Path to the contract build directory.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
}

impl UpParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Deploy a parachain")?;
		Ok(())
	}
}
