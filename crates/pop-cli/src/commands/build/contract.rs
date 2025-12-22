// SPDX-License-Identifier: GPL-3.0

use crate::cli;
use pop_contracts::{MetadataSpec, Verbosity, build_smart_contract};
use std::path::PathBuf;

/// Configuration for building a smart contract.
pub struct BuildContract {
	/// Path of the contract project.
	pub(crate) path: PathBuf,
	/// Build profile: `true` for release mode, `false` for debug mode.
	pub(crate) release: bool,
	/// Which specification to use for contract metadata.
	pub(crate) metadata: Option<MetadataSpec>,
}

impl BuildContract {
	/// Executes the command.
	pub(crate) fn execute(
		self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<serde_json::Value> {
		self.build(cli)
	}

	/// Builds a smart contract
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<serde_json::Value> {
		cli.intro("Building your contract")?;
		// Build contract.
		let build_result =
			build_smart_contract(&self.path, self.release, Verbosity::Default, self.metadata)?;
		cli.success(build_result.display())?;
		cli.outro("Build completed successfully!")?;
		Ok(serde_json::to_value(build_result)?)
	}
}
