// SPDX-License-Identifier: GPL-3.0

use crate::build::spec::GenesisArtifacts;
use anyhow::Result;
use pop_parachains::{ChainSpec, DeploymentProvider, Parachain};
use reqwest::{
	multipart::{Form, Part},
	Client,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// API client for interacting with deployment provider.
pub struct DeploymentApi {
	/// API key used for authentication with the deployment provider.
	pub(crate) api_key: String,
	/// The base URL of the deployment provider's API.
	base_url: String,
	/// HTTP client used for making API requests.
	client: Client,
	/// The selected deployment provider.
	pub(crate) provider: DeploymentProvider,
	/// The chain where the deployment is happening.
	pub(crate) relay_chain_name: String,
}

impl DeploymentApi {
	/// Creates a new API client instance.
	///
	/// # Arguments
	/// * `provider` - The deployment provider to be used.
	/// * `api_key` - The API key used for authentication.
	/// * `relay_chain_name` - The name of the relay chain where deployment will occur.
	pub fn new(
		api_key: String,
		provider: DeploymentProvider,
		relay_chain_name: String,
	) -> Result<Self> {
		Ok(Self {
			api_key,
			base_url: provider.base_url().to_string(),
			client: Client::new(),
			provider,
			relay_chain_name,
		})
	}

	// Creates a new API client instance for testing, allowing to mock the base_url.
	#[cfg(test)]
	fn new_for_testing(
		api_key: String,
		base_url: String,
		provider: DeploymentProvider,
		relay_chain_name: String,
	) -> Result<Self> {
		Ok(Self { api_key, base_url, client: Client::new(), provider, relay_chain_name })
	}

	/// Deploys a parachain by sending the chain specification to the provider.
	///
	/// # Arguments
	/// * `id` - The ID to be deployed.
	/// * `request` - The deployment request containing the necessary parameters.
	pub async fn deploy(&self, id: u32, request: DeployRequest) -> Result<DeployResponse> {
		let url = format!("{}{}", self.base_url, self.provider.get_deploy_path(id));
		let file_part = Part::file(request.chainspec).await?.mime_str("application/json")?;
		let form = Form::new()
			.text("parachainName", request.name)
			.text("signerKey", request.proxy_key)
			.text("runtimeTemplate", request.runtime_template.unwrap_or_default())
			.text("chain", self.relay_chain_name.clone())
			.text("sudoKey", request.sudo_key)
			.text("collatorFileId", request.collator_file_id)
			.part("chainspec", file_part);

		let res = self
			.client
			.post(&url)
			.header("Authorization", format!("Bearer {}", self.api_key))
			.multipart(form)
			.send()
			.await?;
		let status = res.status();
		let body = res.text().await?;
		if !status.is_success() {
			let error_cause = serde_json::from_str::<Value>(&body)
				.ok()
				.and_then(|json| {
					json.get("error")?
						.get("issues")?
						.as_array()?
						.last()?
						.get("message")?
						.as_str()
						.map(|s| s.to_string())
				})
				.unwrap_or(body);

			return Err(anyhow::anyhow!(
				"Deployment failed with status {}: {}",
				status,
				error_cause
			));
		}
		Ok(serde_json::from_str(&body)?)
	}

	/// Retrieves collator keys for a specified parachain.
	///
	/// # Arguments
	/// * `id` - The ID for which collator keys are being fetched.
	pub async fn get_collator_keys(&self, id: u32) -> Result<CollatorKeysResponse> {
		let url = format!(
			"{}{}",
			self.base_url,
			self.provider.get_collator_keys_path(&self.relay_chain_name, id)
		);
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
	pub proxy_key: String,
	/// The runtime template used.
	pub runtime_template: Option<String>,
	/// Sudo account for the created parachain (SS58 format).
	pub sudo_key: String,
	/// Collator file ID.
	pub collator_file_id: String,
	/// Chain specification JSON file (limited to 10 MB).
	pub chainspec: PathBuf,
}
impl DeployRequest {
	/// Creates a new `DeployRequest` by parsing the chain specification file.
	/// # Arguments
	/// * `collator_file_id` - The identifier of the collator file.
	/// * `genesis_artifacts` - Chain specification artifacts.
	/// * `proxy_address` - An optional proxy address in `Id(...)` format.
	pub fn new(
		collator_file_id: String,
		genesis_artifacts: &GenesisArtifacts,
		proxy_address: Option<&str>,
	) -> anyhow::Result<Self> {
		let chain_spec = ChainSpec::from(&genesis_artifacts.chain_spec)?;
		let chain_name = chain_spec
			.get_name()
			.ok_or_else(|| anyhow::anyhow!("Failed to retrieve chain name from the chain spec"))?;
		let proxy_key = proxy_address
			.map(|s| s.trim_start_matches("Id(").trim_end_matches(")"))
			.unwrap_or("")
			.to_string();
		let template = chain_spec
			.get_property_based_on()
			.and_then(Parachain::deployment_name_from_based_on);
		let sudo_address = chain_spec.get_sudo_key().ok_or_else(|| {
			anyhow::anyhow!("Failed to retrieve the sudo address from the chain spec")
		})?;
		Ok(Self {
			name: chain_name.to_string(),
			proxy_key,
			runtime_template: template,
			sudo_key: sudo_address.to_string(),
			collator_file_id,
			chainspec: genesis_artifacts.raw_chain_spec.clone(),
		})
	}
}

/// Response from the deployment call.
#[derive(Debug, Deserialize)]
pub struct DeployResponse {
	/// The status of the deployment.
	pub status: String,
	/// The message returned after deployment.
	#[serde(rename = "rollupUrl")]
	pub message: String,
}

/// Response from a collator keys request.
#[derive(Debug, Deserialize)]
pub struct CollatorKeysResponse {
	/// Collator file ID.
	#[serde(rename = "fileId")]
	pub collator_file_id: String,
	/// List of the collator keys.
	#[serde(rename = "publicCollatorKey", deserialize_with = "deserialize_single_or_vec")]
	pub collator_keys: Vec<String>,
}
fn deserialize_single_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
	D: Deserializer<'de>,
{
	match Value::deserialize(deserializer)? {
		Value::String(s) => Ok(vec![s]),
		Value::Array(arr) => arr
			.into_iter()
			.map(|v| {
				v.as_str()
					.map(String::from)
					.ok_or_else(|| serde::de::Error::custom("Invalid string in array"))
			})
			.collect(),
		_ => Err(serde::de::Error::custom("Invalid format for publicCollatorKey")),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use mockito::{Mock, Server};
	use pop_parachains::SupportedChains;
	use serde_json::json;

	async fn mock_collator_keys(
		mock_server: &mut Server,
		chain_name: &str,
		id: u32,
		payload: &str,
		provider: DeploymentProvider,
	) -> Mock {
		mock_server
			.mock("GET", format!("{}", provider.get_collator_keys_path(chain_name, id)).as_str())
			.with_status(200)
			.with_header("Content-Type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	async fn mock_deploy(
		mock_server: &mut Server,
		para_id: u32,
		payload: &str,
		provider: DeploymentProvider,
	) -> Mock {
		mock_server
			.mock("POST", format!("{}", provider.get_deploy_path(para_id)).as_str())
			.with_status(200)
			.with_header("Content-Type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	async fn mock_deploy_error(
		mock_server: &mut Server,
		para_id: u32,
		provider: DeploymentProvider,
	) -> Mock {
		let mocked_error_payload = json!({
			"error": {
				"issues": [
					{ "message": "Invalid chainspec format: Expected object, received null" },
					{ "message": "ParaId in chainspec (undefined) doesn't match the provided paraId - 2000" }
				]
			}
		})
		.to_string();
		mock_server
			.mock("POST", format!("{}", provider.get_deploy_path(para_id)).as_str())
			.with_status(400)
			.with_header("Content-Type", "application/json")
			.with_body(mocked_error_payload)
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
			"fileId": "1",
			"publicCollatorKey": "0x1234"
		})
		.to_string();
		let id = 2000;
		let mock = mock_collator_keys(
			&mut mock_server,
			&SupportedChains::PASEO.to_string(),
			id,
			&mocked_payload,
			DeploymentProvider::PDP,
		)
		.await;

		let api = DeploymentApi::new_for_testing(
			"api_key".to_string(),
			mock_server.url(),
			DeploymentProvider::PDP,
			SupportedChains::PASEO.to_string(),
		)?;
		let collator_keys = api.get_collator_keys(2000).await?;
		assert_eq!(collator_keys.collator_keys, vec!["0x1234"]);
		assert_eq!(collator_keys.collator_file_id, "1");
		mock.assert_async().await;

		Ok(())
	}

	#[tokio::test]
	async fn deploy_works() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempfile::tempdir()?;
		let mut mock_server = Server::new_async().await;
		let mocked_payload = json!({
			"status": "success",
			"rollupUrl": DeploymentProvider::PDP.base_url()
		})
		.to_string();
		let id = 2000;
		let mock =
			mock_deploy(&mut mock_server, id, &mocked_payload, DeploymentProvider::PDP).await;

		let api = DeploymentApi::new_for_testing(
			"api_key".to_string(),
			mock_server.url(),
			DeploymentProvider::PDP,
			SupportedChains::PASEO.to_string(),
		)?;
		let request = DeployRequest::new(
			"1".to_string(),
			mock_genesis_artifacts(&temp_dir)?,
			Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string()),
		)?;
		let result = api.deploy(2000, request).await?;
		assert_eq!(result.status, "success");
		assert_eq!(result.message, DeploymentProvider::PDP.base_url());
		mock.assert_async().await;

		Ok(())
	}

	#[tokio::test]
	async fn deploy_fails() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempfile::tempdir()?;
		let mut mock_server = Server::new_async().await;
		let id = 2000;
		let mock = mock_deploy_error(&mut mock_server, id, DeploymentProvider::PDP).await;

		let api = DeploymentApi::new_for_testing(
			"api_key".to_string(),
			mock_server.url(),
			DeploymentProvider::PDP,
			SupportedChains::PASEO.to_string(),
		)?;
		let request = DeployRequest::new(
			"1".to_string(),
			mock_genesis_artifacts(&temp_dir)?,
			Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string()),
		)?;
		assert!(
			matches!(api.deploy(2000, request).await, anyhow::Result::Err(message) if message.to_string() == "Deployment failed with status 400 Bad Request: ParaId in chainspec (undefined) doesn't match the provided paraId - 2000")
		);
		mock.assert_async().await;
		Ok(())
	}

	#[test]
	fn new_deploy_request_works() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempfile::tempdir()?;
		let genesis_artifacts = mock_genesis_artifacts(&temp_dir)?;
		let request = DeployRequest::new(
			"1".to_string(),
			genesis_artifacts.clone(),
			Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string()),
		)?;

		assert_eq!(request.name, "Development");
		assert_eq!(
			request.proxy_key,
			"13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf".to_string()
		);
		assert_eq!(request.runtime_template, Some("POP_STANDARD".to_string()));
		assert_eq!(request.sudo_key, "sudo");
		assert_eq!(request.collator_file_id, "1");
		assert_eq!(request.chainspec, genesis_artifacts.raw_chain_spec);
		Ok(())
	}
}
