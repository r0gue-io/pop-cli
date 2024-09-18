// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, APP_USER_AGENT};
use anyhow::Result;
use git2::{
	build::RepoBuilder, FetchOptions, IndexAddOption, RemoteCallbacks, Repository as GitRepository,
	ResetType,
};
use git2_credentials::CredentialHandler;
use regex::Regex;
use std::{fs, path::Path};
use url::Url;

/// A helper for handling Git operations.
pub struct Git;
impl Git {
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
			let object = repo.revparse_single(reference).expect("Object not found");
			repo.checkout_tree(&object, None).expect("Failed to checkout");
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
			Err(_e) =>
				Self::ssh_clone_and_degit(url::Url::parse(url).map_err(Error::from)?, target)?,
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
		Self::parse_latest_tag(tags.iter().flatten().collect::<Vec<_>>())
	}

	/// Parses a list of tags to identify the latest one, prioritizing tags in the stable format
	/// first.
	fn parse_latest_tag(tags: Vec<&str>) -> Option<String> {
		match Self::parse_stable_format(tags.clone()) {
			Some(last_stable_tag) => Some(last_stable_tag),
			None => Self::parse_version_format(tags),
		}
	}

	/// Parse the stable release tags.
	fn parse_stable_format(tags: Vec<&str>) -> Option<String> {
		// Regex for polkadot-stableYYMM and polkadot-stableYYMM-X
		let stable_reg = Regex::new(
			r"polkadot-stable(?P<year>\d{2})(?P<month>\d{2})(-(?P<patch>\d+))?(-rc\d+)?",
		)
		.expect("Valid regex");
		tags.into_iter()
			.filter_map(|tag| {
				// Skip the pre-release label
				if tag.contains("-rc") {
					return None;
				}
				stable_reg.captures(tag).and_then(|v| {
					let year = v.name("year")?.as_str().parse::<u32>().ok()?;
					let month = v.name("month")?.as_str().parse::<u32>().ok()?;
					let patch =
						v.name("patch").and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
					Some((tag, (year, month, patch)))
				})
			})
			.max_by(|a, b| {
				let (_, (year_a, month_a, patch_a)) = a;
				let (_, (year_b, month_b, patch_b)) = b;
				// Compare by year, then by month, then by patch number
				year_a
					.cmp(year_b)
					.then_with(|| month_a.cmp(month_b))
					.then_with(|| patch_a.cmp(patch_b))
			})
			.map(|(tag_str, _)| tag_str.to_string())
	}

	/// Parse the versioning release tags.
	fn parse_version_format(tags: Vec<&str>) -> Option<String> {
		// Regex for polkadot-vmajor.minor.patch format
		let version_reg = Regex::new(r"v(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)(-rc\d+)?")
			.expect("Valid regex");
		tags.into_iter()
			.filter_map(|tag| {
				// Skip the pre-release label
				if tag.contains("-rc") {
					return None;
				}
				version_reg.captures(tag).and_then(|v| {
					let major = v.name("major")?.as_str().parse::<u32>().ok()?;
					let minor = v.name("minor")?.as_str().parse::<u32>().ok()?;
					let patch = v.name("patch")?.as_str().parse::<u32>().ok()?;
					Some((tag, (major, minor, patch)))
				})
			})
			.max_by_key(|&(_, version)| version)
			.map(|(tag_str, _)| tag_str.to_string())
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

/// A helper for handling GitHub operations.
pub struct GitHub {
	pub org: String,
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

	/// Fetch the latest releases of the GitHub repository.
	pub async fn releases(&self) -> Result<Vec<Release>> {
		let client = reqwest::ClientBuilder::new().user_agent(APP_USER_AGENT).build()?;
		let url = self.api_releases_url();
		let response = client.get(url).send().await?.error_for_status()?;
		Ok(response.json::<Vec<Release>>().await?)
	}

	/// Retrieves the commit hash associated with a specified tag in a GitHub repository.
	pub async fn get_commit_sha_from_release(&self, tag_name: &str) -> Result<String> {
		let client = reqwest::ClientBuilder::new().user_agent(APP_USER_AGENT).build()?;
		let response = client
			.get(self.api_tag_information(tag_name))
			.send()
			.await?
			.error_for_status()?;
		let value = response.json::<serde_json::Value>().await?;
		let commit = value
			.get("object")
			.and_then(|v| v.get("sha"))
			.and_then(|v| v.as_str())
			.map(|v| v.to_owned())
			.ok_or(Error::Git("the github release tag sha was not found".to_string()))?;
		Ok(commit)
	}

	pub async fn get_repo_license(&self) -> Result<String> {
		let client = reqwest::ClientBuilder::new().user_agent(APP_USER_AGENT).build()?;
		let url = self.api_license_url();
		let response = client.get(url).send().await?.error_for_status()?;
		let value = response.json::<serde_json::Value>().await?;
		let license = value
			.get("license")
			.and_then(|v| v.get("spdx_id"))
			.and_then(|v| v.as_str())
			.map(|v| v.to_owned())
			.ok_or(Error::Git("Unable to find license for GitHub repo".to_string()))?;
		Ok(license)
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
	pub tag_name: String,
	pub name: String,
	pub prerelease: bool,
	pub commit: Option<String>,
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
	/// Parses a url in the form of https://github.com/org/repository?package#tag into its component parts.
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
			None => crate::GitHub::name(&url)?,
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
			"prerelease": false
		  }]"#;
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(&mock_server.url());
		let mock = releases_mock(&mut mock_server, &repo, expected_payload).await;
		let latest_release = repo.releases().await?;
		assert_eq!(
			latest_release[0],
			Release {
				tag_name: "polkadot-v1.10.0".to_string(),
				name: "Polkadot v1.10.0".into(),
				prerelease: false,
				commit: None
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
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(&mock_server.url());
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
		let repo = GitHub::parse(BASE_PARACHAIN)?.with_api(&mock_server.url());
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
		let url = GitHub::release(&repo, &format!("polkadot-v1.9.0"), "polkadot");
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

	#[test]
	fn parse_latest_tag_works() {
		let mut tags = vec![];
		assert_eq!(Git::parse_latest_tag(tags), None);
		tags = vec![
			"polkadot-stable2407",
			"polkadot-stable2407-1",
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
			"v1.15.1-rc2",
		];
		assert_eq!(Git::parse_latest_tag(tags), Some("polkadot-stable2407-1".to_string()));
	}

	#[test]
	fn parse_stable_format_works() {
		let mut tags = vec![];
		assert_eq!(Git::parse_stable_format(tags), None);
		tags = vec!["polkadot-stable2407", "polkadot-stable2408"];
		assert_eq!(Git::parse_stable_format(tags), Some("polkadot-stable2408".to_string()));
		tags = vec!["polkadot-stable2407", "polkadot-stable2501"];
		assert_eq!(Git::parse_stable_format(tags), Some("polkadot-stable2501".to_string()));
		// Skip the pre-release label
		tags = vec!["polkadot-stable2407", "polkadot-stable2407-1", "polkadot-stable2407-1-rc1"];
		assert_eq!(Git::parse_stable_format(tags), Some("polkadot-stable2407-1".to_string()));
	}

	#[test]
	fn parse_version_format_works() {
		let mut tags: Vec<&str> = vec![];
		assert_eq!(Git::parse_version_format(tags), None);
		tags = vec![
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
		];
		assert_eq!(Git::parse_version_format(tags), Some("polkadot-v1.12.0".to_string()));
		tags = vec!["v1.0.0", "v2.0.0", "v3.0.0"];
		assert_eq!(Git::parse_version_format(tags), Some("v3.0.0".to_string()));
		// Skip the pre-release label
		tags = vec!["polkadot-v1.12.0", "v1.15.1-rc2"];
		assert_eq!(Git::parse_version_format(tags), Some("polkadot-v1.12.0".to_string()));
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
