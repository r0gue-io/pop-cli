// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	common::{
		TestFeature::{self, *},
		contracts::check_contracts_node_and_prompt,
	},
};
use clap::Args;
use cliclack::spinner;
use pop_common::test_project;
use pop_contracts::test_e2e_smart_contract;
use std::path::PathBuf;

const HELP_HEADER: &str = "Smart contract testing options";

#[derive(Args, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub(crate) struct TestContractCommand {
	/// Path to the smart contract.
	#[clap(skip)]
	pub(crate) path: PathBuf,
	/// Run end-to-end tests
	#[arg(short, long)]
	e2e: bool,
	/// Path to the contracts node binary to run e2e tests [default: none]
	#[arg(short, long)]
	node: Option<PathBuf>,
	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
	/// Run with the specified test filter.
	#[arg(skip)]
	pub(crate) test: Option<String>,
}

impl TestContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(
		mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<TestFeature> {
		if self.e2e {
			cli.intro("Starting end-to-end tests")?;
			let spinner = spinner();
			self.node = match check_contracts_node_and_prompt(
				cli,
				&spinner,
				&crate::cache()?,
				self.skip_confirm,
			)
			.await
			{
				Ok(binary_path) => Some(binary_path),
				Err(_) => {
					cli.warning(
						"🚫 substrate-contracts-node is necessary to run e2e tests. Will try to run tests anyway...",
					)?;
					Some(PathBuf::new())
				},
			};

			spinner.clear();
			test_e2e_smart_contract(&self.path, self.node.as_deref(), self.test)?;
			cli.outro("End-to-end testing complete")?;
			Ok(E2e)
		} else {
			cli.intro("Starting unit tests")?;
			test_project(&self.path, self.test)?;
			cli.outro("Unit testing complete")?;
			Ok(Unit)
		}
	}
}
