// SPDX-License-Identifier: GPL-3.0

use crate::{cli::traits::Cli, common::builds::ensure_project_path};
use anyhow::Result;
use clap::{ArgGroup, Args};
use pop_contracts::{DeployedContract, ImageVariant, VerifyContract};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Serialize)]
#[command(
	group = ArgGroup::new("deployed_contract")
		.args(["url", "address", "image"])
		.multiple(true)
		.requires_all(["url", "address", "image"]),
	group = ArgGroup::new("verification_mode")
		.required(true)
		.args(["contract_path", "url"])
)]
pub(crate) struct VerifyCommand {
	/// Directory path with flag for your project directory [default: current directory]
	#[clap(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project  directory [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The reference `.contract` file (`*.contract`) that the selected
	/// contract will be checked against, if verifying against a local project.
	#[arg(short, long, group = "verification_mode")]
	pub(crate) contract_path: Option<PathBuf>,
	/// The URL to the chain where the contract is deployed, if verifying against a deployed
	/// contract. Only chains using latest revive versions are guaranteed to be supported by this
	/// feature, as revive is still in an unstable phase.
	#[arg(short, long, group = "deployed_contract", group = "verification_mode")]
	pub(crate) url: Option<String>,
	/// The address on which the contract is deployed, if verifying against a deployed contract.
	#[arg(short, long, group = "deployed_contract")]
	pub(crate) address: Option<String>,
	/// The image used to compile the deployed contract, if verifying against a deployed contract.
	#[arg(short, long, group = "deployed_contract")]
	pub(crate) image: Option<String>,
}

impl VerifyCommand {
	pub(crate) async fn execute(&self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Start verifying your contract, this may take a bit ⏳")?;

		let project_path = ensure_project_path(self.path.clone(), self.path_pos.clone());

		if let Some(contract_path) = self.contract_path.as_ref() {
			VerifyContract::new_local(project_path, contract_path.clone()).execute().await?;
		} else {
			// SAFETY: clap enforces that if contract_path is not present,
			// then url, address, and image must all be present
			let url = self.url.as_ref().expect("url required by clap");
			let address = self.address.as_ref().expect("address required by clap");
			let image = self.image.as_ref().expect("image required by clap");

			VerifyContract::new_deployed(
				project_path,
				DeployedContract {
					rpc_endpoint: url.clone(),
					contract_address: address.clone(),
					build_image: ImageVariant::from(Some(image.clone())),
				},
			)
			.execute()
			.await?;
		}

		let success_message = if let (Some(endpoint), Some(address)) = (self.url, self.address) {
			format!(
				"The contract deployed in {} at address {} has been succesfully verified ✅",
				endpoint, address
			);
		} else {
			"The contract verification completed successfully ✅".to_string()
		};

		let _ = cli.success(success_message);

		Ok(())
	}
}
