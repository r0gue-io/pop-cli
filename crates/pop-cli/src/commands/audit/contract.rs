use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro, outro};
use pop_contracts::{test_e2e_smart_contract, test_smart_contract};

use crate::style::style;

#[derive(Args)]
pub(crate) struct AuditContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project [default: current directory]")]
	path: Option<PathBuf>,
}

impl AuditContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		// if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
		// 	intro(format!(
		// 		"{}: Starting end-to-end tests",
		// 		style(" Pop CLI ").black().on_magenta()
		// 	))?;
		// 	test_e2e_smart_contract(&self.path)?;
		// 	outro("End-to-end testing complete")?;
		// } else {
		// 	intro(format!("{}: Starting unit tests", style(" Pop CLI ").black().on_magenta()))?;
		// 	test_smart_contract(&self.path)?;
		// 	outro("Unit testing complete")?;
		// }
		Ok(())
	}
}
