// SPDX-License-Identifier: GPL-3.0

use crate::style::style;
use clap::Args;
use cliclack::{clear_screen, confirm, intro, log::warning, outro, spinner};
use pop_contracts::{
	does_contracts_node_exist, download_contracts_node, test_e2e_smart_contract,
	test_smart_contract,
};
use std::path::PathBuf;
use std::{thread::sleep, time::Duration};

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
		if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
			warning("--features e2e-tests is deprecated. Use --e2e instead.")?;
			self.e2e = true;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3));
		}

		if self.e2e {
			intro(format!(
				"{}: Starting end-to-end tests",
				style(" Pop CLI ").black().on_magenta()
			))?;

			// if the contracts node binary does not exist, prompt the user to download it
			let maybe_contract_node_path = does_contracts_node_exist(crate::cache()?);
			if maybe_contract_node_path == None {
				warning("The substrate-contracts-node binary is not found. This is needed to run end-to-end tests.")?;
				if !confirm("Would you like to source the substrate-contracts-node binary?")
					.initial_value(true)
					.interact()?
				{
					warning("🚫 substrate-contracts-node is necessary to run e2e tests. Will try to run tests anyway...")?;
				} else {
					let spinner = spinner();
					spinner.start("Sourcing substrate-contracts-node...");

					let cache_path = crate::cache()?;
					let binary = download_contracts_node(cache_path.clone()).await?;

					spinner.stop(format!(
						"substrate-contracts-node successfully sourced. Cached at: {}",
						binary.path().to_str().unwrap()
					));
					self.node = Some(binary.path());
				}
			} else {
				if let Some(node_path) = maybe_contract_node_path {
					// if the node_path is not empty (cached binary). Otherwise the standalone binary will be used by cargo-contract
					if node_path.0 != PathBuf::new() {
						self.node = Some(node_path.0);
					}
				}
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
