use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro, outro, set_theme};
use console::style;

use crate::{engines::contract_engine::build_smart_contract, style::Theme};

#[derive(Args)]
pub struct BuildContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
}

impl BuildContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building a contract", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		build_smart_contract(&self.path)?;
		outro("Build completed successfully!")?;
		Ok(())
	}
}
