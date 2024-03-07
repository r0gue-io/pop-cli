use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro};

use crate::{engines::contract_engine::test_smart_contract, style::style};

#[derive(Args)]
pub(crate) struct TestContractCommand {
	#[arg(
		short = 'p',
		long = "path",
		help = "Path for the contract project [default: current directory]"
	)]
	path: Option<PathBuf>,
}

impl TestContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Starting unit tests", style(" Pop CLI ").black().on_magenta()))?;
		test_smart_contract(&self.path)?;

		Ok(())
	}
}
