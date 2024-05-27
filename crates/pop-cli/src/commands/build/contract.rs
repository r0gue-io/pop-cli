// SPDX-License-Identifier: GPL-3.0
use clap::Args;
use cliclack::{clear_screen, intro, log, outro, set_theme};
use console::style;
use std::path::PathBuf;

#[cfg(test)]
use crate::mock::build_smart_contract;
use crate::style::Theme;
#[cfg(not(test))]
use pop_contracts::build_smart_contract;

#[derive(Args)]
pub struct BuildContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
}

impl BuildContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building your contract", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		let result_build = build_smart_contract(&self.path)?;
		outro("Build completed successfully!")?;
		log::success(result_build.to_string())?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::new::contract::NewContractCommand;
	use anyhow::{Error, Result};

	async fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_contract_dir = tempfile::tempdir().expect("Could not create temp dir");
		let command = NewContractCommand {
			name: "test_contract".to_string(),
			path: Some(PathBuf::from(temp_contract_dir.path())),
		};
		command.execute().await?;

		Ok(temp_contract_dir)
	}

	#[tokio::test]
	async fn test_build_success() -> Result<()> {
		let temp_dir = setup_test_environment().await?;
		let command = BuildContractCommand {
			path: Some(PathBuf::from(temp_dir.path().join("test_contract"))),
		};

		command.execute()?;
		Ok(())
	}
}
