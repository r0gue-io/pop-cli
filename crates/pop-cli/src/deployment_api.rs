// SPDX-License-Identifier: GPL-3.0

use crate::build::spec::GenesisArtifacts;

use anyhow::Result;
use pop_parachains::{ChainSpec, DeploymentProvider, Parachain, SupportedChains};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// API client for interacting with deployment provider.
pub(crate) struct DeploymentApi {
	/// API key used for authentication with the deployment provider.
	api_key: String,
	/// The base URL of the deployment provider's API.
	base_url: String,
	/// HTTP client used for making API requests.
	client: Client,
	/// The selected deployment provider.
	pub(crate) provider: DeploymentProvider,
}

impl DeploymentApi {
	/// Creates a new API client instance.
	///
	/// # Arguments
	/// * `provider` - The deployment provider to be used (e.g., Polkadot Deployment Portal).
	/// * `api_key` - The API key used for authentication.
	pub fn new(provider: DeploymentProvider, api_key: String) -> Result<Self> {
		Ok(Self {
			api_key,
			base_url: provider.base_url().to_string(),
			client: Client::new(),
			provider,
		})
	}

	// Creates a new API client instance for testing, allowing to mock the base_url.
	#[cfg(test)]
	fn new_for_testing(
		provider: DeploymentProvider,
		api_key: String,
		base_url: String,
	) -> Result<Self> {
		Ok(Self { api_key, base_url, client: Client::new(), provider })
	}

	/// Deploys a parachain by sending the chain specification.
	///
	/// # Arguments
	/// * `id` - The ID for which collator keys are being fetched.
	/// * `request` - The deployment request containing the necessary parameters.
	pub async fn deploy(&self, id: u32, request: DeployRequest) -> Result<DeployResponse> {
		let url = format!("{}{}", self.base_url, self.provider.get_deploy_path(id));
		let res = self
			.client
			.post(&url)
			.header("Authorization", format!("Bearer {}", self.api_key))
			.json(&request)
			.send()
			.await?
			.error_for_status()?;
		Ok(res.json().await?)
	}

	/// Retrieves collator keys for a specified parachain.
	///
	/// # Arguments
	/// * `id` - The ID for which collator keys are being fetched.
	/// * `name` - The name of the chain to be deployed.
	pub async fn get_collator_keys(&self, id: u32, name: &str) -> Result<CollatorKeysResponse> {
		let url = format!("{}{}", self.base_url, self.provider.get_collator_keys_path(name, id));
		let res = self
			.client
			.get(&url)
			.header("Authorization", format!("Bearer {}", self.api_key))
			.send()
			.await?
			.error_for_status()?;
		Ok(res.json().await?)
	}
}

/// Request payload for the deployment call.
#[derive(Debug, Serialize)]
pub struct DeployRequest {
	/// The name of the chain to be deployed.
	pub name: String,
	/// The key of the proxy owner.
	pub proxy_key: Option<String>,
	/// The runtime template used.
	pub runtime_template: Option<String>,
	/// The relay chain where it was registered.
	pub chain: String,
	/// Sudo account for the created parachain (SS58 format).
	pub sudo_key: String,
	/// Collator file ID.
	pub collator_file_id: u32,
	/// Chain specification JSON file (limited to 10 MB).
	pub chainspec: Vec<u8>,
}
impl DeployRequest {
	// Creates a new `DeployRequest`.
	pub fn new(
		collator_file_id: u32,
		genesis_artifacts: GenesisArtifacts,
		relay_chain: Option<SupportedChains>,
		proxy_address: Option<String>,
	) -> anyhow::Result<Self> {
		let chain_spec = ChainSpec::from(&genesis_artifacts.chain_spec)?;
		let chain_name = chain_spec
			.get_name()
			.ok_or_else(|| anyhow::anyhow!("Failed to retrieve chain name from the chain spec"))?;
		let template = chain_spec
			.get_property_based_on()
			.and_then(Parachain::deployment_name_from_based_on)
			.map(String::from);
		let chain = relay_chain
			.ok_or_else(|| anyhow::anyhow!("Failed to retrieve the chain from the chain spec"))?;
		let sudo_address = chain_spec.get_sudo_key().ok_or_else(|| {
			anyhow::anyhow!("Failed to retrieve the sudo address from the chain spec")
		})?;
		let raw_chain_spec = std::fs::read(genesis_artifacts.raw_chain_spec).map_err(|err| {
			anyhow::anyhow!("Failed to read raw_chain_spec file: {}", err.to_string())
		})?;
		Ok(Self {
			name: chain_name.to_string(),
			proxy_key: proxy_address,
			runtime_template: template,
			chain: chain.to_string(),
			sudo_key: sudo_address.to_string(),
			collator_file_id,
			chainspec: raw_chain_spec,
		})
	}
}

/// Response from the deployment call.
#[derive(Debug, Deserialize)]
pub struct DeployResponse {
	pub status: String,
	pub message: String,
}

/// Response from a collator keys request.
#[derive(Debug, Deserialize)]
pub struct CollatorKeysResponse {
	/// Collator file ID.
	pub collator_file_id: u32,
	/// List of the collator keys.
	pub collator_keys: Vec<String>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use mockito::{Mock, Server};
	use serde_json::json;

	async fn mock_collator_keys(
		mock_server: &mut Server,
		id: u32,
		name: &str,
		payload: &str,
	) -> Mock {
		mock_server
			.mock("GET", format!("/public-api/v1/parachains/{}/collators/{}", id, name).as_str())
			.with_status(200)
			.with_header("Content-Type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	async fn mock_deploy(mock_server: &mut Server, para_id: u32, payload: &str) -> Mock {
		mock_server
			.mock("POST", format!("/public-api/v1/parachains/{}/resources", para_id).as_str())
			.with_status(200)
			.with_header("Content-Type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	fn mock_genesis_artifacts(temp_dir: &tempfile::TempDir) -> Result<GenesisArtifacts> {
		let chain_spec = temp_dir.path().join("chain_spec.json");
		std::fs::write(
			&chain_spec,
			json!({
				"name": "Development",
				"properties": {
					"basedOn": "standard",
				},
				"genesis": {
					"runtimeGenesis": {
						"patch": {
							"sudo": {
								"key": "sudo"
							}
						}
					}
				}
			})
			.to_string(),
		)?;
		let raw_chain_spec = temp_dir.path().join("raw_chain_spec.json");
		std::fs::write(&raw_chain_spec, "0x00")?;
		Ok(GenesisArtifacts { chain_spec, raw_chain_spec, ..Default::default() })
	}

	#[tokio::test]
	async fn get_collator_keys_works() -> Result<(), Box<dyn std::error::Error>> {
		let mut mock_server = Server::new_async().await;
		let mocked_payload = json!({
			"collator_file_id": 2000,
			"collator_keys": [ "0x1234", "0x5678" ]
		})
		.to_string();
		let id = 2000;
		let name = "test";
		let mock = mock_collator_keys(&mut mock_server, id, name, &mocked_payload).await;

		let api = DeploymentApi::new_for_testing(
			DeploymentProvider::PDP,
			"api_key".to_string(),
			mock_server.url(),
		)?;
		let collator_keys = api.get_collator_keys(2000, "test").await?;
		assert_eq!(collator_keys.collator_keys, vec!["0x1234", "0x5678"]);
		mock.assert_async().await;

		Ok(())
	}

	#[tokio::test]
	async fn deploy_works() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempfile::tempdir()?;
		let mut mock_server = Server::new_async().await;
		let mocked_payload = json!({
			"status": "success",
			"message": "Deployment successfully"
		})
		.to_string();
		let id = 2000;
		let mock = mock_deploy(&mut mock_server, id, &mocked_payload).await;

		let api = DeploymentApi::new_for_testing(
			DeploymentProvider::PDP,
			"api_key".to_string(),
			mock_server.url(),
		)?;
		let request = DeployRequest::new(
			1,
			mock_genesis_artifacts(&temp_dir)?,
			Some(SupportedChains::PASEO),
			Some("test".to_string()),
		)?;
		let result = api.deploy(2000, request).await?;
		assert_eq!(result.status, "success");
		assert_eq!(result.message, "Deployment successfully");
		mock.assert_async().await;

		Ok(())
	}

	#[test]
	fn new_deploy_request_works() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempfile::tempdir()?;
		let genesis_artifacts = mock_genesis_artifacts(&temp_dir)?;
		let request = DeployRequest::new(
			1,
			genesis_artifacts,
			Some(SupportedChains::PASEO),
			Some("proxy".to_string()),
		)?;

		assert_eq!(request.name, "Development");
		assert_eq!(request.proxy_key, Some("proxy".to_string()));
		assert_eq!(request.runtime_template, Some("standard".to_string()));
		assert_eq!(request.chain, "PASEO");
		assert_eq!(request.sudo_key, "sudo");
		assert_eq!(request.collator_file_id, 1);
		assert_eq!(request.chainspec, "0x00".as_bytes());
		Ok(())
	}
}
