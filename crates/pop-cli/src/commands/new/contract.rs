// SPDX-License-Identifier: GPL-3.0

use std::{env::current_dir, fs, path::PathBuf};

use clap::Args;
use cliclack::{clear_screen, confirm, intro, outro, outro_cancel, set_theme};
use console::style;

use crate::style::Theme;
use pop_contracts::create_smart_contract;

#[derive(Args)]
pub struct NewContractCommand {
	#[arg(help = "Name of the contract")]
	pub(crate) name: String,
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
}

impl NewContractCommand {
	pub(crate) async fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!(
			"{}: Generating new contract \"{}\"!",
			style(" Pop CLI ").black().on_magenta(),
			&self.name,
		))?;
		set_theme(Theme);
		let contract_path = if let Some(ref path) = self.path {
			path.join(&self.name)
		} else {
			current_dir()?.join(&self.name)
		};
		if contract_path.exists() {
			if !confirm(format!(
				"\"{}\" directory already exists. Would you like to remove it?",
				contract_path.display()
			))
			.interact()?
			{
				outro_cancel(format!(
					"Cannot generate contract until \"{}\" directory is removed.",
					contract_path.display()
				))?;
				return Ok(());
			}
			fs::remove_dir_all(contract_path.as_path())?;
		}
		fs::create_dir_all(contract_path.as_path())?;
		let spinner = cliclack::spinner();
		spinner.start("Generating contract...");
		create_smart_contract(&self.name, contract_path.as_path())?;
		spinner.stop("Smart contract created!");
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", contract_path.display()))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[tokio::test]
	async fn test_new_contract_command_execute_success() -> Result<()> {
		let temp_contract_dir = tempfile::tempdir().expect("Could not create temp dir");
		let command = NewContractCommand {
			name: "test_contract".to_string(),
			path: Some(PathBuf::from(temp_contract_dir.path())),
		};
		let result = command.execute().await;
		assert!(result.is_ok());

		Ok(())
	}
}
