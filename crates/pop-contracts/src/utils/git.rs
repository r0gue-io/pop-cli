// SPDX-License-Identifier: GPL-3.0
use crate::errors::Error;
use anyhow::Result;
use git2::{build::RepoBuilder, FetchOptions, RemoteCallbacks, Repository};
use git2_credentials::CredentialHandler;
use std::path::Path;
use url::Url;

/// A helper for handling Git operations.
pub struct Git;
impl Git {
	/// Clone a Git repository.
	///
	/// # Arguments
	///
	/// * `url` - the URL of the repository to clone.
	/// * `target` - location where the repository will be cloned.
	pub fn clone(url: &str, target: &Path) -> Result<()> {
		match Repository::clone(url, target) {
			Ok(repo) => repo,
			Err(_e) => {
				Self::ssh_clone(url::Url::parse(url).map_err(|err| Error::from(err))?, target)?
			},
		};
		Ok(())
	}

	/// For users that have ssh configuration for cloning repositories.
	fn ssh_clone(url: Url, target: &Path) -> Result<Repository> {
		let ssh_url = GitHub::convert_to_ssh_url(&url);
		// Prepare callback and fetch options.
		let mut fo = FetchOptions::new();
		Self::set_up_ssh_fetch_options(&mut fo)?;
		// Prepare builder and clone.
		let mut builder = RepoBuilder::new();
		builder.fetch_options(fo);
		let repo = builder.clone(&ssh_url, target)?;
		Ok(repo)
	}

	fn set_up_ssh_fetch_options(fo: &mut FetchOptions) -> Result<()> {
		let mut callbacks = RemoteCallbacks::new();
		let git_config = git2::Config::open_default()
			.map_err(|e| Error::Config(format!("Cannot open git configuration: {}", e)))?;
		let mut ch = CredentialHandler::new(git_config);
		callbacks.credentials(move |url, username, allowed| {
			ch.try_next_credential(url, username, allowed)
		});

		fo.remote_callbacks(callbacks);
		Ok(())
	}
}
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
	fn convert_to_ssh_url(url: &Url) -> String {
		const GITHUB: &'static str = "github.com";
		format!("git@{}:{}.git", url.host_str().unwrap_or(GITHUB), &url.path()[1..])
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
	const BASE_PARACHAIN: &str = "https://github.com/r0gue-io/base-parachain";

	async fn releases_mock(mock_server: &mut Server, repo: &GitHub, payload: &str) -> Mock {
		mock_server
			.mock("GET", format!("/repos/{}/{}/releases", repo.org, repo.name).as_str())
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	#[test]
	fn test_convert_to_ssh_url() {
		assert_eq!(
			GitHub::convert_to_ssh_url(&Url::parse(BASE_PARACHAIN).expect("valid repository url")),
			"git@github.com:r0gue-io/base-parachain.git"
		);
		assert_eq!(
			GitHub::convert_to_ssh_url(
				&Url::parse("https://github.com/paritytech/substrate-contracts-node")
					.expect("valid repository url")
			),
			"git@github.com:paritytech/substrate-contracts-node.git"
		);
		assert_eq!(
			GitHub::convert_to_ssh_url(
				&Url::parse("https://github.com/paritytech/frontier-parachain-template")
					.expect("valid repository url")
			),
			"git@github.com:paritytech/frontier-parachain-template.git"
		);
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
