// SPDX-License-Identifier: GPL-3.0

use clap::Args;
use cliclack::{clear_screen, intro, set_theme};
use duct::cmd;

use crate::style::{style, Theme};

const BIN_NAME: &str = "substrate-contracts-node";

#[derive(Args)]
pub(crate) struct ContractsNodeCommand;
impl ContractsNodeCommand {
	pub(crate) async fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Launch a contracts node", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		let cache = crate::cache()?;
		let cached_file = cache.join("bin").join(BIN_NAME);
		if !cached_file.exists() {
			cmd(
				"cargo",
				vec!["install", "--root", cache.display().to_string().as_str(), "contracts-node"],
			)
			.run()?;
		}
		cmd(cached_file.display().to_string().as_str(), Vec::<&str>::new()).run()?;
		Ok(())
	}
}
