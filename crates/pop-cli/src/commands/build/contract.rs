// SPDX-License-Identifier: GPL-3.0

use crate::cli;
use clap::Args;
use pop_contracts::{build_smart_contract, Verbosity};
use std::path::PathBuf;

#[derive(Args)]
pub struct BuildContractCommand {
	/// Path for the contract project [default: current directory]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// The default compilation includes debug functionality, increasing contract size and gas
	/// usage. For production, always build in release mode to exclude debug features.
	#[clap(short, long)]
	pub(crate) release: bool,
}

impl BuildContractCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	/// Builds a smart contract
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Building your contract")?;
		// Build contract.
		let build_result =
			build_smart_contract(self.path.as_deref(), self.release, Verbosity::Default)?;
		cli.success(build_result.display())?;
		cli.outro("Build completed successfully!")?;
		Ok("contract")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_contracts::{create_smart_contract, Contract::Standard};
	use std::fs::create_dir_all;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "flipper";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		create_dir_all(path.join(name))?;
		create_smart_contract(name, &path.join(name), &Standard)?;

		for release in [false, true] {
			let mut cli = MockCli::new()
				.expect_intro("Building your contract")
				.expect_outro("Build completed successfully!");

			assert_eq!(
				BuildContractCommand { path: Some(path.join(name)), release }.build(&mut cli)?,
				"contract"
			);

			cli.verify()?;
		}

		Ok(())
	}
}
