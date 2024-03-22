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
