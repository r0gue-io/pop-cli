use anyhow::{anyhow, Result};
use git2::{
	build::RepoBuilder, FetchOptions, IndexAddOption, RemoteCallbacks, Repository, ResetType,
};
use git2_credentials::CredentialHandler;
use regex::Regex;
use std::path::Path;
use std::{env, fs};
use url::Url;

pub struct Git;
impl Git {
	pub(crate) fn clone(url: &Url, working_dir: &Path, branch: Option<&str>) -> Result<()> {
		if !working_dir.exists() {
			let mut fo = FetchOptions::new();
			fo.depth(1);
			let mut repo = RepoBuilder::new();
			repo.fetch_options(fo);
			if let Some(branch) = branch {
				repo.branch(branch);
			}
			if let Err(_e) = repo.clone(url.as_str(), working_dir) {
				Self::ssh_clone(url, working_dir, branch)?;
			}
		}
		Ok(())
	}

	pub(crate) fn ssh_clone(url: &Url, working_dir: &Path, branch: Option<&str>) -> Result<()> {
		// Change the url to the ssh url with git@github.com: prefix, remove / from path and adding .git as suffix
		let ssh_url = ["git@github.com:", &url.path()[1..], ".git"].concat();
		if !working_dir.exists() {
			// Prepare callback and fetch options.
			let mut fo = FetchOptions::new();
			Self::set_up_ssh_fetch_options(&mut fo);
			// Prepare builder and clone.
			let mut repo = RepoBuilder::new();
			repo.fetch_options(fo);
			if let Some(branch) = branch {
				repo.branch(branch);
			}
			repo.clone(&ssh_url, working_dir)?;
		}
		Ok(())
	}
	/// Clone `url` into `target` and degit it
	pub fn clone_and_degit(url: &str, target: &Path) -> Result<Option<String>> {
		let repo = Repository::clone(&["https://github.com/", url].concat(), target)
			.unwrap_or(Self::ssh_clone_and_degit(url, target)?);

		// fetch tags from remote
		let release = Self::fetch_latest_tag(&repo);

		let git_dir = repo.path();
		fs::remove_dir_all(&git_dir)?;
		Ok(release)
	}

	/// For users that have ssh configuration for cloning repositories
	fn ssh_clone_and_degit(url: &str, target: &Path) -> Result<Repository> {
		// Prepare callback and fetch options.
		let mut fo = FetchOptions::new();
		Self::set_up_ssh_fetch_options(&mut fo);
		// Prepare builder and clone.
		let mut builder = RepoBuilder::new();
		builder.fetch_options(fo);
		let repo = builder.clone(&["git@github.com:", url, ".git"].concat(), target)?;
		Ok(repo)
	}

	fn set_up_ssh_fetch_options(fo: &mut FetchOptions) {
		let mut callbacks = RemoteCallbacks::new();
		let git_config = git2::Config::open_default().expect("git configuration cannot open");
		let mut ch = CredentialHandler::new(git_config);
		callbacks.credentials(move |url, username, allowed| {
			ch.try_next_credential(url, username, allowed)
		});

		fo.remote_callbacks(callbacks);
	}

	/// Fetch the latest release from a repository
	fn fetch_latest_tag(repo: &Repository) -> Option<String> {
		let version_reg = Regex::new(r"v\d+\.\d+\.\d+").expect("Valid regex");
		let tags = repo.tag_names(None).ok()?;
		// Start from latest tags
		for tag in tags.iter().rev() {
			if let Some(tag) = tag {
				if version_reg.is_match(tag) {
					return Some(tag.to_string());
				}
			}
		}
		None
	}

	/// Init a new git repo on creation of a parachain
	pub fn git_init(target: &Path, message: &str) -> Result<(), git2::Error> {
		let repo = Repository::init(target)?;
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

pub struct GitHub;
impl GitHub {
	pub async fn get_latest_releases(repo: &Url) -> Result<Vec<Release>> {
		static APP_USER_AGENT: &str =
			concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

		let client = reqwest::ClientBuilder::new().user_agent(APP_USER_AGENT).build()?;
		let response = client
			.get(format!(
				"https://api.github.com/repos/{}/{}/releases",
				Self::org(repo)?,
				Self::name(repo)?
			))
			.send()
			.await?;
		Ok(response.json::<Vec<Release>>().await?)
	}

	fn org(repo: &Url) -> Result<&str> {
		let path_segments = repo
			.path_segments()
			.map(|c| c.collect::<Vec<_>>())
			.expect("repository must have path segments");
		Ok(path_segments
			.get(0)
			.ok_or(anyhow!("the organization (or user) is missing from the github url"))?)
	}

	pub(crate) fn name(repo: &Url) -> Result<&str> {
		let path_segments = repo
			.path_segments()
			.map(|c| c.collect::<Vec<_>>())
			.expect("repository must have path segments");
		Ok(path_segments
			.get(1)
			.ok_or(anyhow!("the repository name is missing from the github url"))?)
	}

	pub(crate) fn release(repo: &Url, tag: &str, artifact: &str) -> String {
		format!("{}/releases/download/{tag}/{artifact}", repo.as_str())
	}
}

#[derive(serde::Deserialize)]
pub struct Release {
	pub(crate) tag_name: String,
	pub(crate) prerelease: bool,
}
