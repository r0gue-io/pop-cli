// SPDX-License-Identifier: GPL-3.0

use crate::{api::ApiClient, errors::Error, polkadot_sdk::parse_latest_tag};
use anyhow::Result;
use git2::{
	FetchOptions, IndexAddOption, RemoteCallbacks, Repository as GitRepository, ResetType,
	build::RepoBuilder,
};
use git2_credentials::CredentialHandler;
use std::{fs, path::Path, sync::LazyLock};
use url::Url;

/// A helper for handling Git operations.
pub struct Git;
impl Git {
	/// Clone a Git repository.
	///
	/// # Arguments
	/// * `url` - the URL of the repository to clone.
	/// * `working_dir` - the target working directory.
	/// * `reference` - an optional reference (revision/tag).
	pub fn clone(url: &Url, working_dir: &Path, reference: Option<&str>) -> Result<()> {
		let mut fo = FetchOptions::new();
		if reference.is_none() {
			fo.depth(1);
		}
		let mut repo = RepoBuilder::new();
		repo.fetch_options(fo);
		let repo = match repo.clone(url.as_str(), working_dir) {
			Ok(repository) => repository,
			Err(e) => match Self::ssh_clone(url, working_dir) {
				Ok(repository) => repository,
				Err(_) => return Err(e.into()),
			},
		};

		if let Some(reference) = reference {
			let object = repo
				.revparse_single(reference)
				.or_else(|_| repo.revparse_single(&format!("refs/tags/{}", reference)))
				.or_else(|_| repo.revparse_single(&format!("refs/remotes/origin/{}", reference)))?;
			repo.checkout_tree(&object, None)?;
			repo.set_head_detached(object.id())?;
		}
		Ok(())
	}

	fn ssh_clone(url: &Url, working_dir: &Path) -> Result<GitRepository> {
		let ssh_url = GitHub::convert_to_ssh_url(url);
		// Prepare callback and fetch options.
		let mut fo = FetchOptions::new();
		Self::set_up_ssh_fetch_options(&mut fo)?;
		// Prepare builder and clone.
		let mut repo = RepoBuilder::new();
		repo.fetch_options(fo);
		Ok(repo.clone(&ssh_url, working_dir)?)
	}

	/// Clone a Git repository and degit it.
	///
	/// # Arguments
	///
	/// * `url` - the URL of the repository to clone.
	/// * `target` - location where the repository will be cloned.
	/// * `tag_version` - the specific tag or version of the repository to use
	pub fn clone_and_degit(
		url: &str,
		target: &Path,
		tag_version: Option<String>,
	) -> Result<Option<String>> {
		let repo = match GitRepository::clone(url, target) {
			Ok(repo) => repo,
			Err(_e) => Self::ssh_clone_and_degit(Url::parse(url).map_err(Error::from)?, target)?,
		};

		if let Some(tag_version) = tag_version {
			let (object, reference) = repo.revparse_ext(&tag_version).expect("Object not found");
			repo.checkout_tree(&object, None).expect("Failed to checkout");
			match reference {
				// gref is an actual reference like branches or tags
				Some(gref) => repo.set_head(gref.name().unwrap()),
				// this is a commit, not a reference
				None => repo.set_head_detached(object.id()),
			}
			.expect("Failed to set HEAD");

			let git_dir = repo.path();
			fs::remove_dir_all(git_dir)?;
			return Ok(Some(tag_version));
		}

		// fetch tags from remote
		let release = Self::fetch_latest_tag(&repo);

		let git_dir = repo.path();
		fs::remove_dir_all(git_dir)?;
		// Or by default the last one
		Ok(release)
	}

	/// For users that have ssh configuration for cloning repositories.
	fn ssh_clone_and_degit(url: Url, target: &Path) -> Result<GitRepository> {
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

	/// Fetch the latest release from a repository
	fn fetch_latest_tag(repo: &GitRepository) -> Option<String> {
		let tags = repo.tag_names(None).ok()?;
		parse_latest_tag(&tags.iter().flatten().collect::<Vec<_>>()).map(|t| t.to_string())
	}

	/// Creates an empty git repository at the specified location.
	///
	/// # Arguments
	///
	/// * `target` - The path where the empty git repository will be initialized.
	pub fn git_create_empty_repository(target: &Path) -> Result<(), git2::Error> {
		GitRepository::init(target)?;
		Ok(())
	}

	/// Init a new git repository.
	///
	/// # Arguments
	///
	/// * `target` - location where the parachain will be created.
	/// * `message` - message for first commit.
	pub fn git_init(target: &Path, message: &str) -> Result<(), git2::Error> {
		let repo = GitRepository::init(target)?;
		let signature = repo.signature()?;

		let mut index = repo.index()?;
		index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
		let tree_id = index.write_tree()?;

		let tree = repo.find_tree(tree_id)?;
		let commit_id = repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?;

		let commit_object = repo.find_object(commit_id, Some(git2::ObjectType::Commit))?;
		repo.reset(&commit_object, ResetType::Hard, None)?;

		Ok(())
	}
}

/// A client for the GitHub REST API.
pub(crate) static GITHUB_API_CLIENT: LazyLock<ApiClient> = LazyLock::new(|| {
	// GitHub API: unauthenticated = 60 requests per hour, authenticated = 5,000 requests per hour,
	// GitHub Actions = 1,000 requests per hour per repository
	ApiClient::new(1, std::env::var("GITHUB_TOKEN").ok())
});

/// A helper for handling GitHub operations.
pub struct GitHub {
	/// The organization name.
	pub org: String,
	/// The repository name
	pub name: String,
	api: String,
}

impl GitHub {
	const GITHUB: &'static str = "github.com";

	/// Parse URL of a GitHub repository.
	///
	/// # Arguments
	///
	/// * `url` - the URL of the repository to clone.
	pub fn parse(url: &str) -> Result<Self> {
		let url = Url::parse(url)?;
		Ok(Self::new(Self::org(&url)?, Self::name(&url)?))
	}

	/// Create a new [GitHub] instance.
	///
	/// # Arguments
	/// * `org` - The organization name.
	/// * `name` - The repository name.
	pub(crate) fn new(org: impl Into<String>, name: impl Into<String>) -> Self {
		Self { org: org.into(), name: name.into(), api: "https://api.github.com".into() }
	}

	/// Overrides the api base URL.
	pub fn with_api(mut self, api: impl Into<String>) -> Self {
		self.api = api.into();
		self
	}

	/// Fetches the latest release of the GitHub repository.
	pub async fn latest_release(&self) -> Result<Release> {
		let url = self.api_latest_release_url();
		let response = GITHUB_API_CLIENT.get(url).await?;
		let release = response.json().await?;
		Ok(release)
	}

	/// Fetch the latest releases of the GitHub repository.
	///
	/// # Arguments
	/// * `prerelease` - Whether to include prereleases.
	pub async fn releases(&self, prerelease: bool) -> Result<Vec<Release>> {
		let url = self.api_releases_url();
		let response = GITHUB_API_CLIENT.get(url).await?;
		let mut releases = response.json::<Vec<Release>>().await?;
		releases.retain(|r| prerelease || !r.prerelease);
		// Sort releases by `published_at` in descending order
		releases.sort_by(|a, b| b.published_at.cmp(&a.published_at));
		Ok(releases)
	}

	/// Retrieves the commit hash associated with a specified tag in a GitHub repository.
	pub async fn get_commit_sha_from_release(&self, tag_name: &str) -> Result<String> {
		let response = GITHUB_API_CLIENT.get(self.api_tag_information(tag_name)).await?;
		let value = response.json::<serde_json::Value>().await?;
		let commit = value
			.get("object")
			.and_then(|v| v.get("sha"))
			.and_then(|v| v.as_str())
			.map(|v| v.to_owned())
			.ok_or(Error::Git("the github release tag sha was not found".to_string()))?;
		Ok(commit)
	}

	/// Retrieves the license from the repository.
	pub async fn get_repo_license(&self) -> Result<String> {
		let url = self.api_license_url();
		let response = GITHUB_API_CLIENT.get(url).await?;
		let value = response.json::<serde_json::Value>().await?;
		let license = value
			.get("license")
			.and_then(|v| v.get("spdx_id"))
			.and_then(|v| v.as_str())
			.map(|v| v.to_owned())
			.ok_or(Error::Git("Unable to find license for GitHub repo".to_string()))?;
		Ok(license)
	}

	fn api_latest_release_url(&self) -> String {
		format!("{}/repos/{}/{}/releases/latest", self.api, self.org, self.name)
	}

	fn api_releases_url(&self) -> String {
		format!("{}/repos/{}/{}/releases", self.api, self.org, self.name)
	}

	fn api_tag_information(&self, tag_name: &str) -> String {
		format!("{}/repos/{}/{}/git/ref/tags/{}", self.api, self.org, self.name, tag_name)
	}

	fn api_license_url(&self) -> String {
		format!("{}/repos/{}/{}/license", self.api, self.org, self.name)
	}

	fn org(repo: &Url) -> Result<&str> {
		let path_segments = repo
			.path_segments()
			.map(|c| c.collect::<Vec<_>>())
			.expect("repository must have path segments");
		Ok(path_segments.first().ok_or(Error::Git(
			"the organization (or user) is missing from the github url".to_string(),
		))?)
	}

	/// Determines the name of a repository from a URL.
	///
	/// # Arguments
	/// * `repo` - the URL of the repository.
	pub fn name(repo: &Url) -> Result<&str> {
		let path_segments = repo
			.path_segments()
			.map(|c| c.collect::<Vec<_>>())
			.expect("repository must have path segments");
		Ok(path_segments
			.get(1)
			.ok_or(Error::Git("the repository name is missing from the github url".to_string()))?)
	}

	#[cfg(test)]
	pub(crate) fn release(repo: &Url, tag: &str, artifact: &str) -> String {
		format!("{}/releases/download/{tag}/{artifact}", repo.as_str())
	}

	pub(crate) fn convert_to_ssh_url(url: &Url) -> String {
		format!("git@{}:{}.git", url.host_str().unwrap_or(Self::GITHUB), &url.path()[1..])
	}
}

/// Represents the data of a GitHub release.
#[derive(Debug, PartialEq, serde::Deserialize)]
pub struct Release {
	/// The name of the tag.
	pub tag_name: String,
	/// The name of the release.
	pub name: String,
	/// Whether to identify the release as a prerelease or a full release.
	pub prerelease: bool,
	/// The commit hash for the release.
	pub commit: Option<String>,
	/// When the release was published.
	pub published_at: String,
}

/// A descriptor of a remote repository.
#[derive(Debug, PartialEq)]
pub struct Repository {
	/// The url of the repository.
	pub url: Url,
	/// If applicable, the branch or tag to be used.
	pub reference: Option<String>,
	/// The name of a package within the repository. Defaults to the repository name.
	pub package: String,
}

impl Repository {
	/// Parses a url in the form of <https://github.com/org/repository?package#tag> into its component parts.
	///
	/// # Arguments
	/// * `url` - The url to be parsed.
	pub fn parse(url: &str) -> Result<Self, Error> {
		let url = Url::parse(url)?;
		let package = url.query();
		let reference = url.fragment().map(|f| f.to_string());

		let mut url = url.clone();
		url.set_query(None);
		url.set_fragment(None);

		let package = match package {
			Some(b) => b,
			None => GitHub::name(&url)?,
		}
		.to_string();

		Ok(Self { url, reference, package })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use mockito::{Mock, Server};

	const BASE_PARACHAIN: &str = "https://github.com/r0gue-io/base-parachain";
	const POLKADOT_SDK: &str = "https://github.com/paritytech/polkadot-sdk";

	async fn latest_release_mock(mock_server: &mut Server, repo: &GitHub, payload: &str) -> Mock {
		mock_server
			.mock("GET", format!("/repos/{}/{}/releases/latest", repo.org, repo.name).as_str())
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	async fn releases_mock(mock_server: &mut Server, repo: &GitHub, payload: &str) -> Mock {
		mock_server
			.mock("GET", format!("/repos/{}/{}/releases", repo.org, repo.name).as_str())
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	async fn tag_mock(mock_server: &mut Server, repo: &GitHub, tag: &str, payload: &str) -> Mock {
		mock_server
			.mock("GET", format!("/repos/{}/{}/git/ref/tags/{tag}", repo.org, repo.name).as_str())
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(payload)
			.create_async()
			.await
	}

	async fn license_mock(mock_server: &mut Server, repo: &GitHub, payload: &str) -> Mock {
		mock_server
			.mock("GET", format!("/repos/{}/{}/license", repo.org, repo.name).as_str())
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
			"tag_name": "polkadot-v1.10.0",
			"name": "Polkadot v1.10.0",
			"prerelease": false,
			"published_at": "2024-01-01T00:00:00Z"
		  },
		  {
			"tag_name": "polkadot-v1.11.0",
			"name": "Polkadot v1.11.0",
			"prerelease": false,
			"published_at": "2023-01-01T00:00:00Z"
		  },
		  {
			"tag_name": "polkadot-v1.12.0",
			"name": "Polkadot v1.12.0",
			"prerelease": false,
			"published_at": "2025-01-01T00:00:00Z"
		  }
		]"#;
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(mock_server.url());
		let mock = releases_mock(&mut mock_server, &repo, expected_payload).await;
		let latest_release = repo.releases(false).await?;
		assert_eq!(
			latest_release,
			vec![
				Release {
					tag_name: "polkadot-v1.12.0".to_string(),
					name: "Polkadot v1.12.0".into(),
					prerelease: false,
					commit: None,
					published_at: "2025-01-01T00:00:00Z".to_string()
				},
				Release {
					tag_name: "polkadot-v1.10.0".to_string(),
					name: "Polkadot v1.10.0".into(),
					prerelease: false,
					commit: None,
					published_at: "2024-01-01T00:00:00Z".to_string()
				},
				Release {
					tag_name: "polkadot-v1.11.0".to_string(),
					name: "Polkadot v1.11.0".into(),
					prerelease: false,
					commit: None,
					published_at: "2023-01-01T00:00:00Z".to_string()
				}
			]
		);
		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn test_get_latest_release() -> Result<(), Box<dyn std::error::Error>> {
		let mut mock_server = Server::new_async().await;

		let expected_payload = r#"{
			"tag_name": "polkadot-v1.12.0",
			"name": "Polkadot v1.12.0",
			"prerelease": false,
			"published_at": "2025-01-01T00:00:00Z"
		  }"#;
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(mock_server.url());
		let mock = latest_release_mock(&mut mock_server, &repo, expected_payload).await;
		let latest_release = repo.latest_release().await?;
		assert_eq!(
			latest_release,
			Release {
				tag_name: "polkadot-v1.12.0".to_string(),
				name: "Polkadot v1.12.0".into(),
				prerelease: false,
				commit: None,
				published_at: "2025-01-01T00:00:00Z".to_string()
			}
		);
		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn get_releases_with_commit_sha() -> Result<(), Box<dyn std::error::Error>> {
		let mut mock_server = Server::new_async().await;

		let expected_payload = r#"{
			"ref": "refs/tags/polkadot-v1.11.0",
			"node_id": "REF_kwDOKDT1SrpyZWZzL3RhZ3MvcG9sa2Fkb3QtdjEuMTEuMA",
			"url": "https://api.github.com/repos/paritytech/polkadot-sdk/git/refs/tags/polkadot-v1.11.0",
			"object": {
				"sha": "0bb6249268c0b77d2834640b84cb52fdd3d7e860",
				"type": "commit",
				"url": "https://api.github.com/repos/paritytech/polkadot-sdk/git/commits/0bb6249268c0b77d2834640b84cb52fdd3d7e860"
			}
		  }"#;
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(mock_server.url());
		let mock = tag_mock(&mut mock_server, &repo, "polkadot-v1.11.0", expected_payload).await;
		let hash = repo.get_commit_sha_from_release("polkadot-v1.11.0").await?;
		assert_eq!(hash, "0bb6249268c0b77d2834640b84cb52fdd3d7e860");
		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn get_repo_license() -> Result<(), Box<dyn std::error::Error>> {
		let mut mock_server = Server::new_async().await;

		let expected_payload = r#"{
			"license": {
			"key":"unlicense",
			"name":"The Unlicense",
			"spdx_id":"Unlicense",
			"url":"https://api.github.com/licenses/unlicense",
			"node_id":"MDc6TGljZW5zZTE1"
			}
		}"#;
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(mock_server.url());
		let mock = license_mock(&mut mock_server, &repo, expected_payload).await;
		let license = repo.get_repo_license().await?;
		assert_eq!(license, "Unlicense".to_string());
		mock.assert_async().await;
		Ok(())
	}

	#[test]
	fn test_get_releases_api_url() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(
			GitHub::parse(POLKADOT_SDK)?.api_releases_url(),
			"https://api.github.com/repos/paritytech/polkadot-sdk/releases"
		);
		Ok(())
	}

	#[test]
	fn test_get_latest_release_api_url() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(
			GitHub::parse(POLKADOT_SDK)?.api_latest_release_url(),
			"https://api.github.com/repos/paritytech/polkadot-sdk/releases/latest"
		);
		Ok(())
	}

	#[test]
	fn test_url_api_tag_information() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(
			GitHub::parse(POLKADOT_SDK)?.api_tag_information("polkadot-v1.11.0"),
			"https://api.github.com/repos/paritytech/polkadot-sdk/git/ref/tags/polkadot-v1.11.0"
		);
		Ok(())
	}

	#[test]
	fn test_api_license_url() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(
			GitHub::parse(POLKADOT_SDK)?.api_license_url(),
			"https://api.github.com/repos/paritytech/polkadot-sdk/license"
		);
		Ok(())
	}

	#[test]
	fn test_parse_org() -> Result<(), Box<dyn std::error::Error>> {
		assert_eq!(GitHub::parse(BASE_PARACHAIN)?.org, "r0gue-io");
		Ok(())
	}

	#[test]
	fn test_parse_name() -> Result<(), Box<dyn std::error::Error>> {
		let url = Url::parse(BASE_PARACHAIN)?;
		let name = GitHub::name(&url)?;
		assert_eq!(name, "base-parachain");
		Ok(())
	}

	#[test]
	fn test_release_url() -> Result<(), Box<dyn std::error::Error>> {
		let repo = Url::parse(POLKADOT_SDK)?;
		let url = GitHub::release(&repo, "polkadot-v1.9.0", "polkadot");
		assert_eq!(url, format!("{}/releases/download/polkadot-v1.9.0/polkadot", POLKADOT_SDK));
		Ok(())
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

	mod repository {
		use super::Error;
		use crate::git::Repository;
		use url::Url;

		#[test]
		fn parsing_full_url_works() {
			assert_eq!(
				Repository::parse("https://github.com/org/repository?package#tag").unwrap(),
				Repository {
					url: Url::parse("https://github.com/org/repository").unwrap(),
					reference: Some("tag".into()),
					package: "package".into(),
				}
			);
		}

		#[test]
		fn parsing_simple_url_works() {
			let url = "https://github.com/org/repository";
			assert_eq!(
				Repository::parse(url).unwrap(),
				Repository {
					url: Url::parse(url).unwrap(),
					reference: None,
					package: "repository".into(),
				}
			);
		}

		#[test]
		fn parsing_invalid_url_returns_error() {
			assert!(matches!(
				Repository::parse("github.com/org/repository"),
				Err(Error::ParseError(..))
			));
		}
	}
}
