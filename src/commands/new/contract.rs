use std::{fs, path::PathBuf};

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
	pub(crate) fn execute(self) -> anyhow::Result<()> {
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
			PathBuf::from(&self.name)
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
		} else {
			fs::create_dir_all(contract_path.as_path())?;
		}
		let mut spinner = cliclack::spinner();
		spinner.start("Generating contract...");

		create_smart_contract(self.name, contract_path.as_path())?;
		spinner.stop("Smart contract created!");
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", contract_path.display()))?;
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
