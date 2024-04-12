use crate::Result;
use anyhow::anyhow;
use git2::{build::RepoBuilder, FetchOptions};
use log::trace;
use std::path::Path;
use url::Url;

pub(crate) struct Git;
impl Git {
	pub(crate) fn clone(url: &Url, working_dir: &Path, branch: Option<&str>) -> Result<()> {
		if !working_dir.exists() {
			trace!("cloning {url}...");
			let mut fo = FetchOptions::new();
			fo.depth(1);
			let mut repo = RepoBuilder::new();
			repo.fetch_options(fo);
			if let Some(branch) = branch {
				repo.branch(branch);
			}
			repo.clone(url.as_str(), working_dir)?;
		}
		Ok(())
	}
}

pub struct GitHub;
impl GitHub {
	#[cfg(feature = "parachain")]
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

#[cfg(feature = "parachain")]
#[derive(serde::Deserialize)]
pub struct Release {
	pub(crate) tag_name: String,
	pub(crate) prerelease: bool,
}
