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

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{clear_screen, intro, outro, set_theme};
use std::path::PathBuf;

use crate::engines::parachain_engine::build_parachain;

#[derive(Args)]
pub struct BuildParachainCommand {
	#[arg(
		short = 'p',
		long = "path",
		help = "Directory path for your project, [default: current directory]"
	)]
	pub(crate) path: Option<PathBuf>,
}

impl BuildParachainCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building a parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);
		build_parachain(&self.path)?;

		outro("Build Completed Successfully!")?;
		Ok(())
	}
}
