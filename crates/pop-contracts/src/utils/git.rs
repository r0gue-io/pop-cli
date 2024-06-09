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
	/// Clone a Git repository and degit it.
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
		let ssh_url = Self::convert_to_ssh_url(&url);
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
	fn convert_to_ssh_url(url: &Url) -> String {
		const GITHUB: &'static str = "github.com";
		format!("git@{}:{}.git", url.host_str().unwrap_or(GITHUB), &url.path()[1..])
	}
}
