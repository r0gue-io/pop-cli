// SPDX-License-Identifier: GPL-3.0

use crate::{common::contracts::check_contracts_node_and_prompt, style::style};
use clap::Args;
use cliclack::{clear_screen, intro, log::warning, outro};
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
}

impl TestContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> anyhow::Result<&'static str> {
		clear_screen()?;

		let mut show_deprecated = false;
		if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
			show_deprecated = true;
			self.e2e = true;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3)).await;
		}

		if self.e2e {
			intro(format!(
				"{}: Starting end-to-end tests",
				style(" Pop CLI ").black().on_magenta()
			))?;

			if show_deprecated {
				warning("NOTE: --features e2e-tests is deprecated. Use --e2e instead.")?;
			}

			let maybe_node_path = check_contracts_node_and_prompt().await?;
			if let Some(node_path) = maybe_node_path {
				if node_path != PathBuf::new() {
					self.node = Some(node_path);
				}
			} else {
				warning("🚫 substrate-contracts-node is necessary to run e2e tests. Will try to run tests anyway...")?;
			}

			test_e2e_smart_contract(self.path.as_deref(), self.node.as_deref())?;
			outro("End-to-end testing complete")?;
			Ok("e2e")
		} else {
			intro(format!("{}: Starting unit tests", style(" Pop CLI ").black().on_magenta()))?;
			test_smart_contract(self.path.as_deref())?;
			outro("Unit testing complete")?;
			Ok("unit")
		}
	}
}
