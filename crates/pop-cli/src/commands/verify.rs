// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::Cli,
	common::builds::ensure_project_path,
	output::{CliResponse, OutputMode},
};
use anyhow::Result;
use clap::{ArgGroup, Args};
use pop_contracts::{DeployedContract, ImageVariant, VerifyContract};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub(crate) struct VerifyOutput {
	verified: bool,
	contract_path: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	image: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	address: Option<String>,
}

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
	fn json_output(
		project_path: &std::path::Path,
		address: Option<&str>,
		image: Option<&str>,
	) -> VerifyOutput {
		VerifyOutput {
			verified: true,
			contract_path: project_path.display().to_string(),
			image: image.map(str::to_string),
			address: address.map(str::to_string),
		}
	}

	pub(crate) async fn execute(&self, cli: &mut impl Cli, output_mode: OutputMode) -> Result<()> {
		cli.intro("Contract verification started. This might take a bit⏳")?;

		let project_path = ensure_project_path(self.path.clone(), self.path_pos.clone());
		let output =
			Self::json_output(&project_path, self.address.as_deref(), self.image.as_deref());

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

		if output_mode == OutputMode::Json {
			CliResponse::ok(output).print_json();
			return Ok(());
		}

		let success_message = if let (Some(endpoint), Some(address)) = (&self.url, &self.address) {
			format!(
				"The contract deployed on {} at address {} is successfully verified ✅",
				endpoint, address
			)
		} else {
			"The contract is successfully verified ✅".to_string()
		};

		let _ = cli.success(success_message);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::output::CliResponse;

	#[test]
	fn verify_output_serializes_for_json() {
		let output = VerifyOutput {
			verified: true,
			contract_path: "contracts/flipper/flipper.contract".to_string(),
			image: Some("use-ink/cargo-contract:latest".to_string()),
			address: Some("0x1234".to_string()),
		};
		let json = serde_json::to_value(CliResponse::ok(output)).unwrap();
		assert_eq!(json["schema_version"], 1);
		assert_eq!(json["success"], true);
		assert_eq!(json["data"]["verified"], true);
		assert_eq!(json["data"]["contract_path"], "contracts/flipper/flipper.contract");
		assert_eq!(json["data"]["image"], "use-ink/cargo-contract:latest");
		assert_eq!(json["data"]["address"], "0x1234");
	}

	#[test]
	fn json_output_uses_project_path_consistently() {
		let project_path = std::path::PathBuf::from("/tmp/flipper");

		let local = VerifyCommand::json_output(&project_path, None, None);
		assert_eq!(local.contract_path, "/tmp/flipper");
		assert_eq!(local.address, None);
		assert_eq!(local.image, None);

		let deployed = VerifyCommand::json_output(
			&project_path,
			Some("0x0000000000000000000000000000000000000001"),
			Some("use-ink/cargo-contract:latest"),
		);
		assert_eq!(deployed.contract_path, "/tmp/flipper");
		assert_eq!(
			deployed.address,
			Some("0x0000000000000000000000000000000000000001".to_string())
		);
		assert_eq!(deployed.image, Some("use-ink/cargo-contract:latest".to_string()));
	}
}
