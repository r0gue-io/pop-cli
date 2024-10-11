// SPDX-License-Identifier: GPL-3.0

use crate::{cli, common::contracts::check_contracts_node_and_prompt};
use clap::Args;
use pop_contracts::{test_e2e_smart_contract, test_smart_contract};
use std::path::PathBuf;
#[cfg(not(test))]
use {std::time::Duration, tokio::time::sleep};

#[derive(Args)]
pub(crate) struct TestContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project [default: current directory]")]
	path: Option<PathBuf>,
	/// [DEPRECATED] Run e2e tests
	#[arg(short = 'f', long = "features", value_parser=["e2e-tests"])]
	features: Option<String>,
	/// Run end-to-end tests
	#[arg(short = 'e', long = "e2e")]
	e2e: bool,
	#[arg(
		short = 'n',
		long = "node",
		help = "Path to the contracts node to run e2e tests [default: none]"
	)]
	node: Option<PathBuf>,
	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short('y'), long)]
	skip_confirm: bool,
}

impl TestContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> anyhow::Result<&'static str> {
		let mut show_deprecated = false;
		if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
			show_deprecated = true;
			self.e2e = true;
		}

		if self.e2e {
			self.execute_e2e_tests(&mut cli::Cli, show_deprecated).await
		} else {
			self.execute_unit_tests(&mut cli::Cli)
		}
	}
	fn execute_unit_tests(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Starting unit tests")?;
		test_smart_contract(self.path.as_deref())?;
		cli.outro("Unit testing complete")?;
		Ok("unit")
	}

	async fn execute_e2e_tests(
		mut self,
		cli: &mut impl cli::traits::Cli,
		show_deprecated: bool,
	) -> anyhow::Result<&'static str> {
		cli.intro("Starting end-to-end tests")?;

		if show_deprecated {
			cli.warning("NOTE: --features e2e-tests is deprecated. Use --e2e instead.")?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3)).await;
		}
		self.node = match check_contracts_node_and_prompt(cli, &crate::cache()?, self.skip_confirm)
			.await
		{
			Ok(binary_path) => Some(binary_path),
			Err(_) => {
				cli.warning("🚫 substrate-contracts-node is necessary to run e2e tests. Will try to run tests anyway...")?;
				Some(PathBuf::new())
			},
		};

		if let Err(e) = test_e2e_smart_contract(self.path.as_deref(), self.node.as_deref()) {
			return Err(anyhow::anyhow!("Failed to run end-to-end tests: {}", e));
		}
		cli.outro("End-to-end testing complete")?;
		Ok("e2e")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;

	#[test]
	fn execute_unit_tests_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;

		let mut cli = MockCli::new()
			.expect_intro("Starting unit tests")
			.expect_outro("Unit testing complete");

		assert_eq!(
			TestContractCommand {
				path: Some(temp_dir.path().join("test_contract")),
				features: None,
				e2e: false,
				node: None,
				skip_confirm: false,
			}
			.execute_unit_tests(&mut cli)?,
			"unit"
		);
		Ok(())
	}

	#[tokio::test]
	async fn execute_e2e_tests_fails_no_contract_with_e2e_feature() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;

		let mut cli = MockCli::new()
			.expect_intro("Starting end-to-end tests")
			.expect_warning("NOTE: --features e2e-tests is deprecated. Use --e2e instead.");

		assert!(matches!(
			TestContractCommand {
				path: Some(temp_dir.path().join("test_contract")),
				features: None,
				e2e: true,
				node: None,
				skip_confirm: false,
			}
			.execute_e2e_tests(&mut cli, true) // To test warning deprecate message
			.await, anyhow::Result::Err(message) if message.to_string().contains("Failed to run end-to-end tests")));
		Ok(())
	}
}
