// Copyright (C) R0GUE IO LTD.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro, outro};

use crate::{
	engines::contract_engine::{test_e2e_smart_contract, test_smart_contract},
	style::style,
};

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
