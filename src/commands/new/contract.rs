use std::{env::current_dir, fs, path::PathBuf};

use clap::Args;
use cliclack::{clear_screen, confirm, intro, outro, outro_cancel, set_theme};
use console::style;

use crate::{engines::contract_engine::create_smart_contract, style::Theme};

#[derive(Args)]
pub struct NewContractCommand {
	#[arg(help = "Name of the contract")]
	pub(crate) name: String,
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
}

impl NewContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!(
			"{}: Generating new contract \"{}\"!",
			style(" Pop CLI ").black().on_magenta(),
			&self.name,
		))?;
		set_theme(Theme);
		let contract_name = self.name.clone();
		let contract_path = self
			.path
			.as_ref()
			.unwrap_or(&current_dir().expect("current dir is inaccessible"))
			.join(contract_name.clone());
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
			fs::remove_dir_all(contract_path)?;
		}
		let mut spinner = cliclack::spinner();
		spinner.start("Generating contract...");

		create_smart_contract(self.name.clone(), &self.path)?;
		spinner.stop(format!(
			"Smart contract created! Located in the following directory {:?}",
			self.path.clone().unwrap_or(PathBuf::from(format!("{}", self.name))).display()
		));
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", &self.name))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn test_new_contract_command_execute_success() -> Result<()> {
		let temp_contract_dir = tempfile::tempdir().expect("Could not create temp dir");
		let command = NewContractCommand {
			name: "test_contract".to_string(),
			path: Some(PathBuf::from(temp_contract_dir.path())),
		};
		let result = command.execute();
		assert!(result.is_ok());

		Ok(())
	}

	#[test]
	fn test_new_contract_command_execute_fails_path_no_exist() -> Result<()> {
		let temp_contract_dir = tempfile::tempdir().expect("Could not create temp dir");
		let command = NewContractCommand {
			name: "test_contract".to_string(),
			path: Some(temp_contract_dir.path().join("new_contract")),
		};
		let result_error = command.execute();
		assert!(result_error.is_err());
		Ok(())
	}
}
