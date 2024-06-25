// SPDX-License-Identifier: GPL-3.0

use crate::style::style;
use clap::Args;
use cliclack::{clear_screen, intro, outro};
use pop_contracts::{test_e2e_smart_contract, test_smart_contract};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct TestContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project [default: current directory]")]
	path: Option<PathBuf>,
	#[arg(short = 'f', long = "features", help = "Features for the contract project", value_parser=["e2e-tests"])]
	features: Option<String>,
	#[arg(
		short = 'n',
		long = "node",
		help = "Path to the contracts node to run e2e tests [default: none]"
	)]
	node: Option<PathBuf>,
}

impl TestContractCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		clear_screen()?;
		if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
			intro(format!(
				"{}: Starting end-to-end tests",
				style(" Pop CLI ").black().on_magenta()
			))?;
			test_e2e_smart_contract(self.path.as_deref(), self.node.as_deref())?;
			outro("End-to-end testing complete")?;
			Ok("e2e")
		} else {
			intro(format!("{}: Starting unit tests", style(" Pop CLI ").black().on_magenta()))?;
			test_smart_contract(self.path.as_deref())?;
			outro("Unit testing complete")?;
			Ok("unit")
		}
	}
}
