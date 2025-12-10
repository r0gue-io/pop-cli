// SPDX-License-Identifier: GPL-3.0

use crate::{cli::traits::Cli, common::builds::PopComposeBuildArgs};
use anyhow::{Context, Result};
use clap::{ArgGroup, Args};
use pop_contracts::{VerifyContract, DeployedContract, ImageVariant};
use regex::Regex;
use serde::Serialize;
use std::{fs::File, path::PathBuf};

#[derive(Args, Serialize)]
#[command(group = ArgGroup::new("deployed_contract")
	.multiple(true)
	.conflicts_with("contract_path")
)]
pub(crate) struct VerifyCommand {
	/// Directory path with flag for your project manifest [default: current directory manifest if
	/// exists]
	#[clap(short, long)]
	manifest_path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory manifest if
	/// exists]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "manifest_path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The reference `.contract` file (`*.contract`) that the selected
	/// contract will be checked against, if verifying against a local project.
	#[arg(short, long)]
	contract_path: Option<PathBuf>,
	/// The URL to the chain where the contract is deployed, if verifying against a deployed
	/// contract.
	#[arg(short, long, group = "deployed_contract", requires_all = ["address", "image"])]
	url: Option<String>,
	/// The address on which the contract is deployed, if verifying against a deployed contract.
	#[arg(short, long, group = "deployed_contract", requires_all = ["url", "image"])]
	address: Option<String>,
	/// The image used to compile the deployed contract, if verifying against a deployed contract.
	#[arg(short, long, group = "deployed_contract", requires_all = ["url", "address"])]
	image: Option<String>,
}

impl VerifyCommand {
	pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Start verifying your contract, this make take a bit ⏳")?;

		let project_path = crate::common::builds::ensure_project_path(
			self.manifest_path.clone(),
			self.path_pos.clone(),
		);

		if let Some(contract_path) = self.contract_path.as_ref() {
			<VerifyContract<PopComposeBuildArgs>>::new_local(project_path, contract_path.clone())
				.execute()?;
		} else if let (Some(url), Some(address), Some(image)) =
			(self.url.as_ref(), self.address.as_ref(), self.image.as_ref())
		{
			<VerifyContract<PopComposeBuildArgs>>::new_deployed(project_path, DeployedContract {
				rpc_endpoint: url.clone(),
				contract_address: address.clone(),
				build_image: ImageVariant::from(Some(image.clone())),
			})
			.execute()?;
		} else {
			anyhow::anyhow!(
				"Either specify a local contract bundle or a deployed contract to verify."
			)?;
		}

		let _ = cli.success("The contract verification completed successfully ✅");

		Ok(())
	}
}
