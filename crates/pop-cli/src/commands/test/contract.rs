// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro, outro};
use pop_contracts::{test_e2e_smart_contract, test_smart_contract};

use crate::style::style;

#[derive(Args)]
pub(crate) struct TestContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project [default: current directory]")]
	path: Option<PathBuf>,
	#[arg(short = 'f', long = "features", help = "Features for the contract project")]
	features: Option<String>,
	#[arg(
		short = 'n',
		long = "contracts-node",
		help = "Path to the contracts node to run the e2e tests [default: none]"
	)]
	contracts_node_path: Option<PathBuf>,
}

impl TestContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<&str> {
		clear_screen()?;

		if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
			intro(format!(
				"{}: Starting end-to-end tests",
				style(" Pop CLI ").black().on_magenta()
			))?;

			test_e2e_smart_contract(&self.path, &self.contracts_node_path)?;
			outro("End-to-end testing complete")?;
			Ok("e2e")
		} else {
			intro(format!("{}: Starting unit tests", style(" Pop CLI ").black().on_magenta()))?;

			test_smart_contract(&self.path)?;
			outro("Unit testing complete")?;
			Ok("unit")
		}
	}
}
