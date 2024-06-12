use anyhow::Result;
use url::Url;

use crate::errors::Error;
/// A helper for handling GitHub operations.
pub struct GitHub {
	pub org: String,
	pub name: String,
	api: String,
}

impl GitHub {
	/// Parse URL of a github repository.
	///
	/// # Arguments
	///
	/// * `url` - the URL of the repository to clone.
	pub fn parse(url: &str) -> Result<Self> {
		let url = Url::parse(url)?;
		Ok(Self {
			org: Self::org(&url)?.into(),
			name: Self::name(&url)?.into(),
			api: "https://api.github.com".into(),
		})
	}

	// Overrides the api base url for testing
	#[cfg(test)]
	fn with_api(mut self, api: impl Into<String>) -> Self {
		self.api = api.into();
		self
	}

	/// Fetch the latest releases of the Github repository.
	pub async fn get_latest_releases(&self) -> Result<Vec<Release>> {
		static APP_USER_AGENT: &str =
			concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
		let client = reqwest::ClientBuilder::new().user_agent(APP_USER_AGENT).build()?;
		let url = self.api_releases_url();
		let response = client.get(url).send().await?;
		Ok(response.json::<Vec<Release>>().await?)
	}

	fn org(repo: &Url) -> Result<&str> {
		let path_segments = repo
			.path_segments()
			.map(|c| c.collect::<Vec<_>>())
			.expect("repository must have path segments");
		Ok(path_segments.get(0).ok_or(Error::Git(
			"the organization (or user) is missing from the github url".to_string(),
		))?)
	}

	pub(crate) fn name(repo: &Url) -> Result<&str> {
		let path_segments = repo
			.path_segments()
			.map(|c| c.collect::<Vec<_>>())
			.expect("repository must have path segments");
		Ok(path_segments
			.get(1)
			.ok_or(Error::Git("the repository name is missing from the github url".to_string()))?)
	}

	fn api_releases_url(&self) -> String {
		format!("{}/repos/{}/{}/releases", self.api, self.org, self.name)
	}
}

/// Represents the data of a GitHub release.
#[derive(Debug, PartialEq, serde::Deserialize)]
pub struct Release {
	pub tag_name: String,
	pub name: String,
	pub prerelease: bool,
	pub commit: Option<String>,
}
#[cfg(test)]
mod tests {
	use super::*;
	use mockito::{Mock, Server};

	const SUBSTRATE_CONTRACT_NODE: &str = "https://github.com/paritytech/substrate-contracts-node";

	async fn releases_mock(mock_server: &mut Server, repo: &GitHub, payload: &str) -> Mock {
		mock_server
			.mock("GET", format!("/repos/{}/{}/releases", repo.org, repo.name).as_str())
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	#[tokio::test]
	async fn test_get_latest_releases() -> Result<(), Box<dyn std::error::Error>> {
		let mut mock_server = Server::new_async().await;

		let expected_payload = r#"[{
			"tag_name": "v0.41.0",
			"name": "v0.41.0",
			"prerelease": false
		  }]"#;
		let repo = GitHub::parse(SUBSTRATE_CONTRACT_NODE)?.with_api(&mock_server.url());
		let mock = releases_mock(&mut mock_server, &repo, expected_payload).await;
		let latest_release = repo.get_latest_releases().await?;
		assert_eq!(
			latest_release[0],
			Release {
				tag_name: "v0.41.0".to_string(),
				name: "v0.41.0".into(),
				prerelease: false,
				commit: None
			}
		);
		mock.assert_async().await;
		Ok(())
	}

	#[test]
	fn test_parse_org() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(GitHub::parse(SUBSTRATE_CONTRACT_NODE)?.org, "paritytech");
		Ok(())
	}

	#[test]
	fn test_parse_name() -> Result<(), Box<dyn std::error::Error>> {
		let url = Url::parse(SUBSTRATE_CONTRACT_NODE)?;
		let name = GitHub::name(&url)?;
		assert_eq!(name, "substrate-contracts-node");
		Ok(())
	}

	#[test]
	fn test_get_releases_api_url() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(
			GitHub::parse(SUBSTRATE_CONTRACT_NODE)?.api_releases_url(),
			"https://api.github.com/repos/paritytech/substrate-contracts-node/releases"
		);
		Ok(())
	}
}
