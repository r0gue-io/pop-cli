// SPDX-License-Identifier: GPL-3.0

use crate::{api, git::GITHUB_API_CLIENT, Git, Status};
pub use binary::*;
use duct::cmd;
use flate2::read::GzDecoder;
use reqwest::StatusCode;
use std::{
	error::Error as _,
	fs::{copy, metadata, read_dir, rename, File},
	io::{BufRead, Seek, SeekFrom, Write},
	os::unix::fs::PermissionsExt,
	path::{Path, PathBuf},
	time::Duration,
};
use tar::Archive;
use tempfile::{tempdir, tempfile};
use thiserror::Error;
use url::Url;

mod binary;

/// An error relating to the sourcing of binaries.
#[derive(Error, Debug)]
pub enum Error {
	/// An error occurred.
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	/// An API error occurred.
	#[error("API error: {0}")]
	ApiError(#[from] api::Error),
	/// An error occurred sourcing a binary from an archive.
	#[error("Archive error: {0}")]
	ArchiveError(String),
	/// A HTTP error occurred.
	#[error("HTTP error: {0} caused by {:?}", reqwest::Error::source(.0))]
	HttpError(#[from] reqwest::Error),
	/// An IO error occurred.
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	/// A binary cannot be sourced.
	#[error("Missing binary: {0}")]
	MissingBinary(String),
	/// An error occurred during parsing.
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
}

/// The source of a binary.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
	/// An archive for download.
	#[allow(dead_code)]
	Archive {
		/// The url of the archive.
		url: String,
		/// The archive contents required, including the binary name.
		contents: Vec<String>,
	},
	/// A git repository.
	Git {
		/// The url of the repository.
		url: Url,
		/// If applicable, the branch, tag or commit.
		reference: Option<String>,
		/// If applicable, a specification of the path to the manifest.
		manifest: Option<PathBuf>,
		/// The name of the package to be built.
		package: String,
		/// Any additional build artifacts which are required.
		artifacts: Vec<String>,
	},
	/// A GitHub repository.
	GitHub(GitHub),
	/// A URL for download.
	#[allow(dead_code)]
	Url {
		/// The URL for download.
		url: String,
		/// The name of the binary.
		name: String,
	},
}

impl Source {
	/// Sources the binary.
	///
	/// # Arguments
	///
	/// * `cache` - the cache to be used.
	/// * `release` - whether any binaries needing to be built should be done so using the release
	///   profile.
	/// * `status` - used to observe status updates.
	/// * `verbose` - whether verbose output is required.
	pub(super) async fn source(
		&self,
		cache: &Path,
		release: bool,
		status: &impl Status,
		verbose: bool,
	) -> Result<(), Error> {
		use Source::*;
		match self {
			Archive { url, contents } => {
				let contents: Vec<_> =
					contents.iter().map(|name| (name.as_str(), cache.join(name))).collect();
				from_archive(url, &contents, status).await
			},
			Git { url, reference, manifest, package, artifacts } => {
				let artifacts: Vec<_> = artifacts
					.iter()
					.map(|name| match reference {
						Some(version) => (name.as_str(), cache.join(format!("{name}-{version}"))),
						None => (name.as_str(), cache.join(name)),
					})
					.collect();
				from_git(
					url.as_str(),
					reference.as_deref(),
					manifest.as_ref(),
					package,
					&artifacts,
					release,
					status,
					verbose,
				)
				.await
			},
			GitHub(source) => source.source(cache, release, status, verbose).await,
			Url { url, name } => from_url(url, &cache.join(name), status).await,
		}
	}
}

/// A binary sourced from GitHub.
#[derive(Clone, Debug, PartialEq)]
pub enum GitHub {
	/// An archive for download from a GitHub release.
	ReleaseArchive {
		/// The owner of the repository - i.e. <https://github.com/{owner}/repository>.
		owner: String,
		/// The name of the repository - i.e. <https://github.com/owner/{repository}>.
		repository: String,
		/// The release tag to be used, where `None` is latest.
		tag: Option<String>,
		/// If applicable, any formatting for the release tag.
		tag_format: Option<String>,
		/// The name of the archive (asset) to download.
		archive: String,
		/// The archive contents required, including the binary name.
		/// The second parameter can be used to specify another name for the binary once extracted.
		contents: Vec<(&'static str, Option<String>)>,
		/// If applicable, the latest release tag available.
		latest: Option<String>,
	},
	/// A source code archive for download from GitHub.
	SourceCodeArchive {
		/// The owner of the repository - i.e. <https://github.com/{owner}/repository>.
		owner: String,
		/// The name of the repository - i.e. <https://github.com/owner/{repository}>.
		repository: String,
		/// If applicable, the branch, tag or commit.
		reference: Option<String>,
		/// If applicable, a specification of the path to the manifest.
		manifest: Option<PathBuf>,
		/// The name of the package to be built.
		package: String,
		/// Any additional artifacts which are required.
		artifacts: Vec<String>,
	},
}

impl GitHub {
	/// Sources the binary.
	///
	/// # Arguments
	///
	/// * `cache` - the cache to be used.
	/// * `release` - whether any binaries needing to be built should be done so using the release
	///   profile.
	/// * `status` - used to observe status updates.
	/// * `verbose` - whether verbose output is required.
	async fn source(
		&self,
		cache: &Path,
		release: bool,
		status: &impl Status,
		verbose: bool,
	) -> Result<(), Error> {
		use GitHub::*;
		match self {
			ReleaseArchive { owner, repository, tag, tag_format, archive, contents, .. } => {
				// Complete url and contents based on tag
				let base_url = format!("https://github.com/{owner}/{repository}/releases");
				let url = match tag.as_ref() {
					Some(tag) => {
						let tag = tag_format.as_ref().map_or_else(
							|| tag.to_string(),
							|tag_format| tag_format.replace("{tag}", tag),
						);
						format!("{base_url}/download/{tag}/{archive}")
					},
					None => format!("{base_url}/latest/download/{archive}"),
				};
				let contents: Vec<_> = contents
					.iter()
					.map(|(name, target)| match tag.as_ref() {
						Some(tag) => (
							*name,
							cache.join(format!(
								"{}-{tag}",
								target.as_ref().map_or(*name, |t| t.as_str())
							)),
						),
						None => (*name, cache.join(target.as_ref().map_or(*name, |t| t.as_str()))),
					})
					.collect();
				from_archive(&url, &contents, status).await
			},
			SourceCodeArchive { owner, repository, reference, manifest, package, artifacts } => {
				let artifacts: Vec<_> = artifacts
					.iter()
					.map(|name| match reference {
						Some(reference) =>
							(name.as_str(), cache.join(format!("{name}-{reference}"))),
						None => (name.as_str(), cache.join(name)),
					})
					.collect();
				from_github_archive(
					owner,
					repository,
					reference.as_ref().map(|r| r.as_str()),
					manifest.as_ref(),
					package,
					&artifacts,
					release,
					status,
					verbose,
				)
				.await
			},
		}
	}
}

/// Source binary by downloading and extracting from an archive.
///
/// # Arguments
/// * `url` - The url of the archive.
/// * `contents` - The contents within the archive which are required.
/// * `status` - Used to observe status updates.
async fn from_archive(
	url: &str,
	contents: &[(&str, PathBuf)],
	status: &impl Status,
) -> Result<(), Error> {
	// Download archive
	status.update(&format!("Downloading from {url}..."));
	let response = reqwest::get(url).await?.error_for_status()?;
	let mut file = tempfile()?;
	file.write_all(&response.bytes().await?)?;
	file.seek(SeekFrom::Start(0))?;
	// Extract contents
	status.update("Extracting from archive...");
	let tar = GzDecoder::new(file);
	let mut archive = Archive::new(tar);
	let temp_dir = tempdir()?;
	let working_dir = temp_dir.path();
	archive.unpack(working_dir)?;
	for (name, dest) in contents {
		let src = working_dir.join(name);
		if src.exists() {
			if let Err(_e) = rename(&src, dest) {
				// If rename fails (e.g., due to cross-device linking), fallback to copy and remove
				copy(&src, dest)?;
				std::fs::remove_file(&src)?;
			}
		} else {
			return Err(Error::ArchiveError(format!(
				"Expected file '{}' in archive, but it was not found.",
				name
			)));
		}
	}
	status.update("Sourcing complete.");
	Ok(())
}

/// Source binary by cloning a git repository and then building.
///
/// # Arguments
/// * `url` - The url of the repository.
/// * `reference` - If applicable, the branch, tag or commit.
/// * `manifest` - If applicable, a specification of the path to the manifest.
/// * `package` - The name of the package to be built.
/// * `artifacts` - Any additional artifacts which are required.
/// * `release` - Whether to build optimized artifacts using the release profile.
/// * `status` - Used to observe status updates.
/// * `verbose` - Whether verbose output is required.
#[allow(clippy::too_many_arguments)]
async fn from_git(
	url: &str,
	reference: Option<&str>,
	manifest: Option<impl AsRef<Path>>,
	package: &str,
	artifacts: &[(&str, impl AsRef<Path>)],
	release: bool,
	status: &impl Status,
	verbose: bool,
) -> Result<(), Error> {
	// Clone repository into working directory
	let temp_dir = tempdir()?;
	let working_dir = temp_dir.path();
	status.update(&format!("Cloning {url}..."));
	Git::clone(&Url::parse(url)?, working_dir, reference)?;
	// Build binaries
	status.update("Starting build of binary...");
	let manifest = manifest
		.as_ref()
		.map_or_else(|| working_dir.join("Cargo.toml"), |m| working_dir.join(m));
	build(manifest, package, artifacts, release, status, verbose).await?;
	status.update("Sourcing complete.");
	Ok(())
}

/// Source binary by downloading from a source code archive and then building.
///
/// # Arguments
/// * `owner` - The owner of the repository.
/// * `repository` - The name of the repository.
/// * `reference` - If applicable, the branch, tag or commit.
/// * `manifest` - If applicable, a specification of the path to the manifest.
/// * `package` - The name of the package to be built.
/// * `artifacts` - Any additional artifacts which are required.
/// * `release` - Whether to build optimized artifacts using the release profile.
/// * `status` - Used to observe status updates.
/// * `verbose` - Whether verbose output is required.
#[allow(clippy::too_many_arguments)]
async fn from_github_archive(
	owner: &str,
	repository: &str,
	reference: Option<&str>,
	manifest: Option<impl AsRef<Path>>,
	package: &str,
	artifacts: &[(&str, impl AsRef<Path>)],
	release: bool,
	status: &impl Status,
	verbose: bool,
) -> Result<(), Error> {
	// User agent required when using GitHub API
	let response =
		match reference {
			Some(reference) => {
				// Various potential urls to try based on not knowing the type of ref
				let urls = [
					format!("https://github.com/{owner}/{repository}/archive/refs/heads/{reference}.tar.gz"),
					format!("https://github.com/{owner}/{repository}/archive/refs/tags/{reference}.tar.gz"),
					format!("https://github.com/{owner}/{repository}/archive/{reference}.tar.gz"),
				];
				let mut response = None;
				for url in urls {
					status.update(&format!("Downloading from {url}..."));
					response = Some(GITHUB_API_CLIENT.get(url).await);
					if let Some(Err(api::Error::HttpError(e))) = &response {
						if e.status() == Some(StatusCode::NOT_FOUND) {
							tokio::time::sleep(Duration::from_secs(1)).await;
							continue;
						}
					}
					break;
				}
				response.expect("value set above")?
			},
			None => {
				let url = format!("https://api.github.com/repos/{owner}/{repository}/tarball");
				status.update(&format!("Downloading from {url}..."));
				GITHUB_API_CLIENT.get(url).await?
			},
		};
	let mut file = tempfile()?;
	file.write_all(&response)?;
	file.seek(SeekFrom::Start(0))?;
	// Extract contents
	status.update("Extracting from archive...");
	let tar = GzDecoder::new(file);
	let mut archive = Archive::new(tar);
	let temp_dir = tempdir()?;
	let mut working_dir = temp_dir.path().into();
	archive.unpack(&working_dir)?;
	// Prepare archive contents for build
	let entries: Vec<_> = read_dir(&working_dir)?.take(2).filter_map(|x| x.ok()).collect();
	match entries.len() {
		0 =>
			return Err(Error::ArchiveError(
				"The downloaded archive does not contain any entries.".into(),
			)),
		1 => working_dir = entries[0].path(), // Automatically switch to top level directory
		_ => {},                              /* Assume that downloaded archive does not have a
		                                        * top level directory */
	}
	// Build binaries
	status.update("Starting build of binary...");
	let manifest = manifest
		.as_ref()
		.map_or_else(|| working_dir.join("Cargo.toml"), |m| working_dir.join(m));
	build(&manifest, package, artifacts, release, status, verbose).await?;
	status.update("Sourcing complete.");
	Ok(())
}

/// Source binary by building a local package.
///
/// # Arguments
/// * `manifest` - The path to the local package manifest.
/// * `package` - The name of the package to be built.
/// * `release` - Whether to build optimized artifacts using the release profile.
/// * `status` - Used to observe status updates.
/// * `verbose` - Whether verbose output is required.
pub(crate) async fn from_local_package(
	manifest: &Path,
	package: &str,
	release: bool,
	status: &impl Status,
	verbose: bool,
) -> Result<(), Error> {
	// Build binaries
	status.update("Starting build of binary...");
	const EMPTY: [(&str, PathBuf); 0] = [];
	build(manifest, package, &EMPTY, release, status, verbose).await?;
	status.update("Sourcing complete.");
	Ok(())
}

/// Source binary by downloading from a url.
///
/// # Arguments
/// * `url` - The url of the binary.
/// * `path` - The (local) destination path.
/// * `status` - Used to observe status updates.
async fn from_url(url: &str, path: &Path, status: &impl Status) -> Result<(), Error> {
	// Download required version of binaries
	status.update(&format!("Downloading from {url}..."));
	download(url, path).await?;
	status.update("Sourcing complete.");
	Ok(())
}

/// Builds a package.
///
/// # Arguments
/// * `manifest` - The path to the manifest.
/// * `package` - The name of the package to be built.
/// * `artifacts` - Any additional artifacts which are required.
/// * `release` - Whether to build optimized artifacts using the release profile.
/// * `status` - Used to observe status updates.
/// * `verbose` - Whether verbose output is required.
async fn build(
	manifest: impl AsRef<Path>,
	package: &str,
	artifacts: &[(&str, impl AsRef<Path>)],
	release: bool,
	status: &impl Status,
	verbose: bool,
) -> Result<(), Error> {
	// Define arguments
	let manifest_path = manifest.as_ref().to_str().expect("expected manifest path to be valid");
	let mut args = vec!["build", "-p", package, "--manifest-path", manifest_path];
	if release {
		args.push("--release")
	}
	// Build binaries
	let command = cmd("cargo", args);
	match verbose {
		false => {
			let reader = command.stderr_to_stdout().reader()?;
			let output = std::io::BufReader::new(reader).lines();
			for line in output {
				status.update(&line?);
			}
		},
		true => {
			command.run()?;
		},
	}
	// Copy required artifacts to destination path
	let target = manifest
		.as_ref()
		.parent()
		.expect("")
		.join(format!("target/{}", if release { "release" } else { "debug" }));
	for (name, dest) in artifacts {
		copy(target.join(name), dest)?;
	}
	Ok(())
}

/// Downloads a file from a URL.
///
/// # Arguments
/// * `url` - The url of the file.
/// * `path` - The (local) destination path.
async fn download(url: &str, dest: &Path) -> Result<(), Error> {
	// Download to destination path
	let response = reqwest::get(url).await?.error_for_status()?;
	let mut file = File::create(dest)?;
	file.write_all(&response.bytes().await?)?;
	// Make executable
	set_executable_permission(dest)?;
	Ok(())
}

/// Sets the executable permission for a given file.
///
/// # Arguments
/// * `path` - The file path to which permissions should be granted.
pub fn set_executable_permission<P: AsRef<Path>>(path: P) -> Result<(), Error> {
	let mut perms = metadata(&path)?.permissions();
	perms.set_mode(0o755);
	std::fs::set_permissions(path, perms)?;
	Ok(())
}

#[cfg(test)]
pub(super) mod tests {
	use super::{GitHub::*, Status, *};
	use crate::target;
	use tempfile::tempdir;

	#[tokio::test]
	async fn sourcing_from_archive_works() -> anyhow::Result<()> {
		let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz".to_string();
		let name = "polkadot".to_string();
		let contents =
			vec![name.clone(), "polkadot-execute-worker".into(), "polkadot-prepare-worker".into()];
		let temp_dir = tempdir()?;

		Source::Archive { url, contents: contents.clone() }
			.source(temp_dir.path(), true, &Output, true)
			.await?;
		for item in contents {
			assert!(temp_dir.path().join(item).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_git_works() -> anyhow::Result<()> {
		let url = Url::parse("https://github.com/hpaluch/rust-hello-world")?;
		let package = "hello_world".to_string();
		let temp_dir = tempdir()?;

		Source::Git {
			url,
			reference: None,
			manifest: None,
			package: package.clone(),
			artifacts: vec![package.clone()],
		}
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		assert!(temp_dir.path().join(package).exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_git_ref_works() -> anyhow::Result<()> {
		let url = Url::parse("https://github.com/hpaluch/rust-hello-world")?;
		let initial_commit = "436b7dbffdfaaf7ad90bf44ae8fdcb17eeee65a3".to_string();
		let package = "hello_world".to_string();
		let temp_dir = tempdir()?;

		Source::Git {
			url,
			reference: Some(initial_commit.clone()),
			manifest: None,
			package: package.clone(),
			artifacts: vec![package.clone()],
		}
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		assert!(temp_dir.path().join(format!("{package}-{initial_commit}")).exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_github_release_archive_works() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let tag = "v1.12.0";
		let tag_format = Some("polkadot-{tag}".to_string());
		let name = "polkadot".to_string();
		let archive = format!("{name}-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;

		Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: Some(tag.to_string()),
			tag_format,
			archive,
			contents: contents.map(|n| (n, None)).to_vec(),
			latest: None,
		})
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		for item in contents {
			assert!(temp_dir.path().join(format!("{item}-{tag}")).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_github_release_archive_maps_contents() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let tag = "v1.12.0";
		let tag_format = Some("polkadot-{tag}".to_string());
		let name = "polkadot".to_string();
		let archive = format!("{name}-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;
		let prefix = "test";

		Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: Some(tag.to_string()),
			tag_format,
			archive,
			contents: contents.map(|n| (n, Some(format!("{prefix}-{n}")))).to_vec(),
			latest: None,
		})
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		for item in contents {
			assert!(temp_dir.path().join(format!("{prefix}-{item}-{tag}")).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_latest_github_release_archive_works() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let tag_format = Some("polkadot-{tag}".to_string());
		let name = "polkadot".to_string();
		let archive = format!("{name}-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;

		Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: None,
			tag_format,
			archive,
			contents: contents.map(|n| (n, None)).to_vec(),
			latest: None,
		})
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		for item in contents {
			assert!(temp_dir.path().join(item).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_github_source_code_archive_works() -> anyhow::Result<()> {
		let owner = "paritytech".to_string();
		let repository = "polkadot-sdk".to_string();
		let package = "polkadot".to_string();
		let temp_dir = tempdir()?;
		let initial_commit = "72dba98250a6267c61772cd55f8caf193141050f";
		let manifest = PathBuf::from("substrate/Cargo.toml");

		Source::GitHub(SourceCodeArchive {
			owner,
			repository,
			reference: Some(initial_commit.to_string()),
			manifest: Some(manifest),
			package: package.clone(),
			artifacts: vec![package.clone()],
		})
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		assert!(temp_dir.path().join(format!("{package}-{initial_commit}")).exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_latest_github_source_code_archive_works() -> anyhow::Result<()> {
		let owner = "hpaluch".to_string();
		let repository = "rust-hello-world".to_string();
		let package = "hello_world".to_string();
		let temp_dir = tempdir()?;

		Source::GitHub(SourceCodeArchive {
			owner,
			repository,
			reference: None,
			manifest: None,
			package: package.clone(),
			artifacts: vec![package.clone()],
		})
		.source(temp_dir.path(), true, &Output, true)
		.await?;
		assert!(temp_dir.path().join(package).exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_url_works() -> anyhow::Result<()> {
		let url =
			"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc"
				.to_string();
		let name = "polkadot";
		let temp_dir = tempdir()?;

		Source::Url { url, name: name.into() }
			.source(temp_dir.path(), false, &Output, true)
			.await?;
		assert!(temp_dir.path().join(&name).exists());
		Ok(())
	}

	#[tokio::test]
	async fn from_archive_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz";
		let contents: Vec<_> = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"]
			.into_iter()
			.map(|b| (b, temp_dir.path().join(b)))
			.collect();

		from_archive(url, &contents, &Output).await?;
		for (_, file) in contents {
			assert!(file.exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn from_git_works() -> anyhow::Result<()> {
		let url = "https://github.com/hpaluch/rust-hello-world";
		let package = "hello_world";
		let initial_commit = "436b7dbffdfaaf7ad90bf44ae8fdcb17eeee65a3";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(package);

		from_git(
			url,
			Some(initial_commit),
			None::<&Path>,
			&package,
			&[(&package, &path)],
			true,
			&Output,
			false,
		)
		.await?;
		assert!(path.exists());
		Ok(())
	}

	#[tokio::test]
	async fn from_github_archive_works() -> anyhow::Result<()> {
		let owner = "paritytech";
		let repository = "polkadot-sdk";
		let package = "polkadot";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(package);
		let initial_commit = "72dba98250a6267c61772cd55f8caf193141050f";
		let manifest = "substrate/Cargo.toml";

		from_github_archive(
			owner,
			repository,
			Some(initial_commit),
			Some(manifest),
			package,
			&[(package, &path)],
			true,
			&Output,
			true,
		)
		.await?;
		assert!(path.exists());
		Ok(())
	}

	#[tokio::test]
	async fn from_latest_github_archive_works() -> anyhow::Result<()> {
		let owner = "hpaluch";
		let repository = "rust-hello-world";
		let package = "hello_world";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(package);

		from_github_archive(
			owner,
			repository,
			None,
			None::<&Path>,
			package,
			&[(package, &path)],
			true,
			&Output,
			true,
		)
		.await?;
		assert!(path.exists());
		Ok(())
	}

	#[tokio::test]
	async fn from_local_package_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let name = "hello_world";
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let manifest = temp_dir.path().join(name).join("Cargo.toml");

		from_local_package(&manifest, name, false, &Output, true).await?;
		assert!(manifest.parent().unwrap().join("target/debug").join(name).exists());
		Ok(())
	}

	#[tokio::test]
	async fn from_url_works() -> anyhow::Result<()> {
		let url =
			"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join("polkadot");

		from_url(url, &path, &Output).await?;
		assert!(path.exists());
		assert_ne!(metadata(path)?.permissions().mode() & 0o755, 0);
		Ok(())
	}

	pub(crate) struct Output;
	impl Status for Output {
		fn update(&self, status: &str) {
			println!("{status}")
		}
	}
}

/// Traits for the sourcing of a binary.
pub mod traits {
	use crate::{sourcing::Error, GitHub};
	use strum::EnumProperty;

	/// The source of a binary.
	pub trait Source: EnumProperty {
		/// The name of the binary.
		fn binary(&self) -> &'static str {
			self.get_str("Binary").expect("expected specification of `Binary` name")
		}

		/// The fallback version to be used when the latest version cannot be determined.
		fn fallback(&self) -> &str {
			self.get_str("Fallback")
				.expect("expected specification of `Fallback` release tag")
		}

		/// Whether pre-releases are to be used.
		fn prerelease(&self) -> Option<bool> {
			self.get_str("Prerelease")
				.map(|v| v.parse().expect("expected parachain prerelease value to be true/false"))
		}

		/// Determine the available releases from the source.
		#[allow(async_fn_in_trait)]
		async fn releases(&self) -> Result<Vec<String>, Error> {
			let repo = GitHub::parse(self.repository())?;
			let releases = match repo.releases().await {
				Ok(releases) => releases,
				Err(_) => return Ok(vec![self.fallback().to_string()]),
			};
			let prerelease = self.prerelease();
			let tag_format = self.tag_format();
			Ok(releases
				.iter()
				.filter(|r| match prerelease {
					None => !r.prerelease, // Exclude pre-releases by default
					Some(prerelease) => r.prerelease == prerelease,
				})
				.map(|r| {
					if let Some(tag_format) = tag_format {
						// simple for now, could be regex in future
						let tag_format = tag_format.replace("{tag}", "");
						r.tag_name.replace(&tag_format, "")
					} else {
						r.tag_name.clone()
					}
				})
				.collect())
		}

		/// The repository to be used.
		fn repository(&self) -> &str {
			self.get_str("Repository").expect("expected specification of `Repository` url")
		}

		/// If applicable, any tag format to be used - e.g. `polkadot-{tag}`.
		fn tag_format(&self) -> Option<&str> {
			self.get_str("TagFormat")
		}
	}

	/// An attempted conversion into a Source.
	pub trait TryInto {
		/// Attempt the conversion.
		///
		/// # Arguments
		/// * `specifier` - If applicable, some specifier used to determine a specific source.
		/// * `latest` - If applicable, some specifier used to determine the latest source.
		fn try_into(
			&self,
			specifier: Option<String>,
			latest: Option<String>,
		) -> Result<super::Source, crate::Error>;
	}

	#[cfg(test)]
	mod tests {
		use super::Source;
		use strum_macros::{EnumProperty, VariantArray};

		#[derive(EnumProperty, VariantArray)]
		pub(super) enum Chain {
			#[strum(props(
				Repository = "https://github.com/paritytech/polkadot-sdk",
				Binary = "polkadot",
				Prerelease = "false",
				Fallback = "v1.12.0",
				TagFormat = "polkadot-{tag}"
			))]
			Polkadot,
			#[strum(props(
				Repository = "https://github.com/r0gue-io/fallback",
				Fallback = "v1.0"
			))]
			Fallback,
		}

		impl Source for Chain {}

		#[test]
		fn binary_works() {
			assert_eq!("polkadot", Chain::Polkadot.binary())
		}

		#[test]
		fn fallback_works() {
			assert_eq!("v1.12.0", Chain::Polkadot.fallback())
		}

		#[test]
		fn prerelease_works() {
			assert!(!Chain::Polkadot.prerelease().unwrap())
		}

		#[tokio::test]
		async fn releases_works() -> anyhow::Result<()> {
			assert!(!Chain::Polkadot.releases().await?.is_empty());
			Ok(())
		}

		#[tokio::test]
		async fn releases_uses_fallback() -> anyhow::Result<()> {
			let chain = Chain::Fallback;
			assert_eq!(chain.fallback(), chain.releases().await?[0]);
			Ok(())
		}

		#[test]
		fn repository_works() {
			assert_eq!("https://github.com/paritytech/polkadot-sdk", Chain::Polkadot.repository())
		}

		#[test]
		fn tag_format_works() {
			assert_eq!("polkadot-{tag}", Chain::Polkadot.tag_format().unwrap())
		}
	}
}
