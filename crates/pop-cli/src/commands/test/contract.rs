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
}

impl TestContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
			intro(format!(
				"{}: Starting end-to-end tests",
				style(" Pop CLI ").black().on_magenta()
			))?;
			test_e2e_smart_contract(&self.path)?;
			outro("End-to-end testing complete")?;
		} else {
			intro(format!("{}: Starting unit tests", style(" Pop CLI ").black().on_magenta()))?;
			test_smart_contract(&self.path)?;
			outro("Unit testing complete")?;
		}
		Ok(())
	}
}
