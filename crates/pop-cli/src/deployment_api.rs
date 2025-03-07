// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use pop_parachains::DeploymentProvider;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// API client for interacting with deployment provider.
pub struct DeploymentApi {
	/// API key used for authentication with the deployment provider.
	api_key: String,
	/// The base URL of the deployment provider's API.
	base_url: String,
	/// HTTP client used for making API requests.
	client: Client,
	/// The selected deployment provider.
	provider: DeploymentProvider,
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
	pub runtime_template: String,
    /// The relay chain where it was registered.
    pub chain: String,
    /// Sudo account for the created parachain (SS58 format).
    pub sudo_key: String,
    /// Collator file ID.
    pub collator_file_id: String,
    /// Chain specification JSON file (limited to 10 MB).
    pub chainspec: Vec<u8>,
}

/// Response from the deployment call.
#[derive(Debug, Deserialize)]
pub struct DeployResponse {
	pub status: String,
	pub message: Option<String>,
}

/// Response from a collator keys request.
#[derive(Debug, Deserialize)]
pub struct CollatorKeysResponse {
    /// List of the collator keys.
	pub collator_keys: Vec<String>,
    /// Collator file ID.
    pub collator_file_id: String,
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

	#[tokio::test]
	async fn get_collator_keys_works() -> Result<(), Box<dyn std::error::Error>> {
		let mut mock_server = Server::new_async().await;
		let mocked_payload = json!({
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
        let request = DeployRequest {
            name: "test".to_string(),
            proxy_key: None,
            runtime_template: "test".to_string(),
            chain: "test".to_string(),
            sudo_key: "test".to_string(),
            collator_file_id: "test".to_string(),
            chainspec: vec![1, 2, 3],
        };
		let result = api.deploy(2000, request).await?;
		assert_eq!(result.status, "success");
        assert_eq!(result.message, Some("Deployment successfully".to_string()));
		mock.assert_async().await;

		Ok(())
	}
}
