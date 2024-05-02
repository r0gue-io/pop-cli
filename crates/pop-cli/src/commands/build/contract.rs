// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro, log, outro, set_theme};
use console::style;

use crate::style::Theme;
use pop_contracts::build_smart_contract;

#[derive(Args)]
pub struct BuildContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
}

impl BuildContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building a contract", style(" Pop CLI ").black().on_magenta()))?;

		tokio::spawn(pop_telemetry::record_cli_command(
			"build",
			serde_json::json!({"contract": ""}),
		));

		set_theme(Theme);

		let result_build = build_smart_contract(&self.path)?;
		outro("Build completed successfully!")?;
		log::success(result_build.to_string())?;
		Ok(())
	}
}
