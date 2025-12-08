// SPDX-License-Identifier: GPL-3.0

use crate::{Git, Release, SortedSlice, Status, api, git::GITHUB_API_CLIENT};
pub use archive::{ALLOWED_FILE_EXTENSIONS, ArchiveType, SourcedArchive};
use derivative::Derivative;
use duct::cmd;
use flate2::read::GzDecoder;
use regex::Regex;
use reqwest::StatusCode;
use std::{
	collections::HashMap,
	error::Error as _,
	fs::{File, copy, metadata, read_dir, rename},
	io::{BufRead, Seek, SeekFrom, Write},
	os::unix::fs::PermissionsExt,
	path::{Path, PathBuf},
	time::Duration,
};
use tar::Archive;
use tempfile::{tempdir, tempfile};
use thiserror::Error;
use url::Url;

mod archive;

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
	/// An archive cannot be sourced.
	#[error("Missing archive: {0}")]
	MissingArchive(String),
	/// An error occurred during parsing.
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
}

/// The source of an archive
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
		/// Any additional build artifacts that are required.
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
	/// Sources the archive.
	///
	/// # Arguments
	/// * `cache` - the cache to be used.
	/// * `release` - whether any binaries needing to be built should be done so using the release
	///   profile.
	/// * `status` - used to observe status updates.
	/// * `verbose` - whether verbose output is required.
	/// * `archive_type` - the type of the archive.
	pub(super) async fn source(
		&self,
		cache: &Path,
		release: bool,
		status: &impl Status,
		verbose: bool,
		archive_type: ArchiveType,
	) -> Result<(), Error> {
		use Source::*;
		match self {
			Archive { url, contents } => {
				let contents: Vec<_> = contents
					.iter()
					.map(|name| ArchiveFileSpec::new(name.into(), Some(cache.join(name)), true))
					.collect();
				from_archive(url, &contents, status, archive_type).await
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
			GitHub(source) => source.source(cache, release, status, verbose, archive_type).await,
			Url { url, name } => from_url(url, &cache.join(name), status, archive_type).await,
		}
	}

	/// Performs any additional processing required to resolve the binary from a source.
	///
	/// Determines whether the binary already exists locally, using the latest version available,
	/// and whether there are any newer versions available
	///
	/// # Arguments
	/// * `name` - the name of the binary.
	/// * `version` - a specific version of the binary required.
	/// * `cache` - the cache being used.
	/// * `cache_filter` - a filter to be used to determine whether a cached binary is eligible.
	pub async fn resolve(
		self,
		name: &str,
		version: Option<&str>,
		cache: &Path,
		cache_filter: impl for<'a> FnOnce(&'a str) -> bool + Copy,
	) -> Self {
		match self {
			Source::GitHub(github) =>
				Source::GitHub(github.resolve(name, version, cache, cache_filter).await),
			_ => self,
		}
	}
}

/// A binary sourced from GitHub.
#[derive(Clone, Debug, Derivative)]
#[derivative(PartialEq)]
pub enum GitHub {
	/// An archive for download from a GitHub release.
	ReleaseArchive {
		/// The owner of the repository - i.e. <https://github.com/{owner}/repository>.
		owner: String,
		/// The name of the repository - i.e. <https://github.com/owner/{repository}>.
		repository: String,
		/// The release tag to be used, where `None` is latest.
		tag: Option<String>,
		/// If applicable, a pattern to be used to determine applicable releases along with
		/// determining subcomponents from a release tag - e.g. `polkadot-{version}`.
		tag_pattern: Option<TagPattern>,
		/// Whether pre-releases are to be used.
		prerelease: bool,
		/// A function that orders candidates for selection when multiple versions are available.
		#[derivative(PartialEq = "ignore")]
		version_comparator: for<'a> fn(&'a mut [String]) -> SortedSlice<'a, String>,
		/// The version to use if an appropriate version cannot be resolved.
		fallback: String,
		/// The name of the archive (asset) to download.
		archive: String,
		/// The archive contents required.
		contents: Vec<ArchiveFileSpec>,
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
		/// Any additional artifacts that are required.
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
	/// * `archive_type` - The archive type
	async fn source(
		&self,
		cache: &Path,
		release: bool,
		status: &impl Status,
		verbose: bool,
		archive_type: ArchiveType,
	) -> Result<(), Error> {
		use GitHub::*;
		match self {
			ReleaseArchive { owner, repository, tag, tag_pattern, archive, contents, .. } => {
				// Complete url and contents based on the tag
				let base_url = format!("https://github.com/{owner}/{repository}/releases");
				let url = match tag.as_ref() {
					Some(tag) => {
						format!("{base_url}/download/{tag}/{archive}")
					},
					None => format!("{base_url}/latest/download/{archive}"),
				};
				let contents: Vec<_> = contents
					.iter()
					.map(|ArchiveFileSpec { name, target, required }| match tag.as_ref() {
						Some(tag) => ArchiveFileSpec::new(
							name.into(),
							Some(cache.join(format!(
									"{}-{}",
									target.as_ref().map_or(name.as_str(), |t| t
										.to_str()
										.expect("expected target file name to be valid utf-8")),
									tag_pattern
										.as_ref()
										.and_then(|pattern| pattern.version(tag))
										.unwrap_or(tag)
								))),
							*required,
						),
						None => ArchiveFileSpec::new(
							name.into(),
							Some(cache.join(target.as_ref().map_or(name.as_str(), |t| {
								t.to_str().expect("expected target file name to be valid utf-8")
							}))),
							*required,
						),
					})
					.collect();
				from_archive(&url, &contents, status, archive_type).await
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

	/// Performs any additional processing required to resolve the binary from a source.
	///
	/// Determines whether the binary already exists locally, using the latest version available,
	/// and whether there are any newer versions available
	///
	/// # Arguments
	/// * `name` - the name of the binary.
	/// * `version` - a specific version of the binary required.
	/// * `cache` - the cache being used.
	/// * `cache_filter` - a filter to be used to determine whether a cached binary is eligible.
	async fn resolve(
		self,
		name: &str,
		version: Option<&str>,
		cache: &Path,
		cache_filter: impl FnOnce(&str) -> bool + Copy,
	) -> Self {
		match self {
			Self::ReleaseArchive {
				owner,
				repository,
				tag: _,
				tag_pattern,
				prerelease,
				version_comparator,
				fallback,
				archive,
				contents,
				latest: _,
			} => {
				// Get releases, defaulting to the specified fallback version if there's an error.
				let repo = crate::GitHub::new(owner.as_str(), repository.as_str());
				let mut releases = repo.releases(prerelease).await.unwrap_or_else(|_e| {
					// Use any specified version or fall back to the last known version.
					let version = version.unwrap_or(fallback.as_str());
					vec![Release {
						tag_name: tag_pattern.as_ref().map_or_else(
							|| version.to_string(),
							|pattern| pattern.resolve_tag(version),
						),
						name: String::default(),
						prerelease,
						commit: None,
						published_at: String::default(),
					}]
				});

				// Filter releases if a tag pattern specified
				if let Some(pattern) = tag_pattern.as_ref() {
					releases.retain(|r| pattern.regex.is_match(&r.tag_name));
				}

				// Select versions from release tags, used for resolving the candidate versions and
				// local binary versioning.
				let mut binaries: HashMap<_, _> = releases
					.into_iter()
					.map(|r| {
						let version = tag_pattern
							.as_ref()
							.and_then(|pattern| pattern.version(&r.tag_name).map(|v| v.to_string()))
							.unwrap_or_else(|| r.tag_name.clone());
						(version, r.tag_name)
					})
					.collect();

				// Resolve any specified version - i.e., the version could be provided as a concrete
				// version or just a tag.
				let version = version.map(|v| {
					tag_pattern
						.as_ref()
						.and_then(|pattern| pattern.version(v))
						.unwrap_or(v)
						.to_string()
				});

				// Extract versions from any cached binaries - e.g., offline or rate-limited.
				let cached_files = read_dir(cache).into_iter().flatten();
				let cached_file_names = cached_files
					.filter_map(|f| f.ok().and_then(|f| f.file_name().into_string().ok()));
				for file in cached_file_names.filter(|f| cache_filter(f)) {
					let mut version = file.replace(&format!("{name}-"), "");
					ALLOWED_FILE_EXTENSIONS.iter().for_each(|ext| {
						version = version.strip_suffix(ext).unwrap_or(&version).to_owned();
					});
					let tag = tag_pattern.as_ref().map_or_else(
						|| version.to_string(),
						|pattern| pattern.resolve_tag(&version),
					);
					binaries.insert(version, tag);
				}

				// Prepare for version resolution by sorting by configured version comparator.
				let mut versions: Vec<_> = binaries.keys().cloned().collect();
				let versions = version_comparator(versions.as_mut_slice());

				// Define the tag to be used as either a specified version or the latest available
				// locally.
				let tag = version.as_ref().map_or_else(
					|| {
						// Resolve the version to be used.
						let resolved_version =
							SourcedArchive::resolve_version(name, None, &versions, cache);
						resolved_version.and_then(|v| binaries.get(v)).cloned()
					},
					|v| {
						// Ensure any specified version is a tag.
						Some(
							tag_pattern
								.as_ref()
								.map_or_else(|| v.to_string(), |pattern| pattern.resolve_tag(v)),
						)
					},
				);

				// // Default to the latest version when no specific version is provided by the
				// caller.
				let latest: Option<String> = version
					.is_none()
					.then(|| versions.first().and_then(|v| binaries.get(v.as_str()).cloned()))
					.flatten();

				Self::ReleaseArchive {
					owner,
					repository,
					tag,
					tag_pattern,
					prerelease,
					version_comparator,
					fallback,
					archive,
					contents,
					latest,
				}
			},
			_ => self,
		}
	}
}

/// A specification of a file within an archive.
#[derive(Clone, Debug, PartialEq)]
pub struct ArchiveFileSpec {
	/// The name of the file within the archive.
	pub name: String,
	/// An optional file name to be used for the file once extracted.
	pub target: Option<PathBuf>,
	/// Whether the file is required.
	pub required: bool,
}

impl ArchiveFileSpec {
	/// A specification of a file within an archive.
	///
	/// # Arguments
	/// * `name` - The name of the file within the archive.
	/// * `target` - An optional file name to be used for the file once extracted.
	/// * `required` - Whether the file is required.
	pub fn new(name: String, target: Option<PathBuf>, required: bool) -> Self {
		Self { name, target, required }
	}
}

/// A pattern used to determine captures from a release tag.
///
/// Only `{version}` is currently supported, used to determine a version from a release tag.
/// Examples: `polkadot-{version}`, `node-{version}`.
#[derive(Clone, Debug)]
pub struct TagPattern {
	regex: Regex,
	pattern: String,
}

impl TagPattern {
	/// A new pattern used to determine captures from a release tag.
	///
	/// # Arguments
	/// * `pattern` - the pattern to be used.
	pub fn new(pattern: &str) -> Self {
		Self {
			regex: Regex::new(&format!("^{}$", pattern.replace("{version}", "(?P<version>.+)")))
				.expect("expected valid regex"),
			pattern: pattern.into(),
		}
	}

	/// Resolves a tag for the specified value.
	///
	/// # Arguments
	/// * `value` - the value to resolve into a tag using the inner tag pattern.
	pub fn resolve_tag(&self, value: &str) -> String {
		// If input already in expected tag format, return as-is.
		if self.regex.is_match(value) {
			return value.to_string();
		}

		self.pattern.replace("{version}", value)
	}

	/// Extracts a version from the specified value.
	///
	/// # Arguments
	/// * `value` - the value to parse.
	pub fn version<'a>(&self, value: &'a str) -> Option<&'a str> {
		self.regex.captures(value).and_then(|c| c.name("version").map(|v| v.as_str()))
	}
}

impl PartialEq for TagPattern {
	fn eq(&self, other: &Self) -> bool {
		self.regex.as_str() == other.regex.as_str() && self.pattern == other.pattern
	}
}

impl From<&str> for TagPattern {
	fn from(value: &str) -> Self {
		Self::new(value)
	}
}

/// Source binary by downloading and extracting from an archive.
///
/// # Arguments
/// * `url` - The url of the archive.
/// * `contents` - The contents within the archive which are required.
/// * `status` - Used to observe status updates.
/// * `archive_type` - Whether the archive is a file or a binary
async fn from_archive(
	url: &str,
	contents: &[ArchiveFileSpec],
	status: &impl Status,
	archive_type: ArchiveType,
) -> Result<(), Error> {
	// Download archive
	status.update(&format!("Downloading from {url}..."));
	let response = reqwest::get(url).await?.error_for_status()?;
	let mut file = tempfile()?;
	file.write_all(&response.bytes().await?)?;
	file.seek(SeekFrom::Start(0))?;
	match archive_type {
		ArchiveType::Binary => {
			// Extract contents from tar.gz archive
			status.update("Extracting from archive...");
			let tar = GzDecoder::new(file);
			let mut archive = Archive::new(tar);
			let temp_dir = tempdir()?;
			let working_dir = temp_dir.path();
			archive.unpack(working_dir)?;
			for ArchiveFileSpec { name, target, required } in contents {
				let src = working_dir.join(name);
				if src.exists() {
					set_executable_permission(&src)?;
					if let Some(target) = target &&
						let Err(_e) = rename(&src, target)
					{
						// If rename fails (e.g., due to cross-device linking), fallback to copy and
						// remove
						copy(&src, target)?;
						std::fs::remove_file(&src)?;
					}
				} else if *required {
					return Err(Error::ArchiveError(format!(
						"Expected file '{}' in archive, but it was not found.",
						name
					)));
				}
			}
		},
		ArchiveType::File => {
			if let Some(ArchiveFileSpec { name, target: Some(target), .. }) = contents.first() {
				let final_target = if let Some(ext) = Path::new(name).extension() {
					PathBuf::from(format!("{}.{}", target.display(), ext.to_string_lossy()))
				} else {
					target.to_path_buf()
				};
				let mut target_file = File::create(&final_target)?;
				std::io::copy(&mut file, &mut target_file)?;
			} else {
				return Err(Error::ArchiveError(
					"File archive requires exactly one target path".to_owned(),
				));
			}
		},
	}

	status.update("Sourcing complete.");
	Ok(())
}

/// Source a binary by cloning a git repository and then building.
///
/// # Arguments
/// * `url` - The url of the repository.
/// * `reference` - If applicable, the branch, tag or commit.
/// * `manifest` - If applicable, a specification of the path to the manifest.
/// * `package` - The name of the package to be built.
/// * `artifacts` - Any additional artifacts that are required.
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
/// * `artifacts` - Any additional artifacts that are required.
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
	let response = match reference {
		Some(reference) => {
			// Various potential urls to try based on not knowing the type of ref
			let urls = [
				format!(
					"https://github.com/{owner}/{repository}/archive/refs/heads/{reference}.tar.gz"
				),
				format!(
					"https://github.com/{owner}/{repository}/archive/refs/tags/{reference}.tar.gz"
				),
				format!("https://github.com/{owner}/{repository}/archive/{reference}.tar.gz"),
			];
			let mut response = None;
			for url in urls {
				status.update(&format!("Downloading from {url}..."));
				response = Some(GITHUB_API_CLIENT.get(url).await);
				if let Some(Err(api::Error::HttpError(e))) = &response &&
					e.status() == Some(StatusCode::NOT_FOUND)
				{
					tokio::time::sleep(Duration::from_secs(1)).await;
					continue;
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
		0 => {
			return Err(Error::ArchiveError(
				"The downloaded archive does not contain any entries.".into(),
			));
		},
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

/// Source binary by downloading from a URL.
///
/// # Arguments
/// * `url` - The url of the binary.
/// * `path` - The (local) destination path.
/// * `status` - Used to observe status updates.
/// * `archive-type` - Archive type
async fn from_url(
	url: &str,
	path: &Path,
	status: &impl Status,
	archive_type: ArchiveType,
) -> Result<(), Error> {
	// Download the binary
	status.update(&format!("Downloading from {url}..."));
	download(url, path, archive_type).await?;
	status.update("Sourcing complete.");
	Ok(())
}

/// Builds a package.
///
/// # Arguments
/// * `manifest` - The path to the manifest.
/// * `package` - The name of the package to be built.
/// * `artifacts` - Any additional artifacts that are required.
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
	// Copy required artifacts to the destination path
	let target = manifest
		.as_ref()
		.parent()
		.expect("expected parent directory to be valid")
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
/// * `archive_type` - The archive type.
async fn download(url: &str, dest: &Path, archive_type: ArchiveType) -> Result<(), Error> {
	// Download to the destination path
	let response = reqwest::get(url).await?.error_for_status()?;
	let mut file = File::create(dest)?;
	file.write_all(&response.bytes().await?)?;
	// Make executable if binary
	if let ArchiveType::Binary = archive_type {
		set_executable_permission(dest)?;
	}
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
	use crate::{polkadot_sdk::parse_version, target};
	use tempfile::tempdir;

	#[tokio::test]
	async fn sourcing_from_archive_works() -> anyhow::Result<()> {
		let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz".to_string();
		let name = "polkadot".to_string();
		let contents =
			vec![name.clone(), "polkadot-execute-worker".into(), "polkadot-prepare-worker".into()];
		let temp_dir = tempdir()?;

		Source::Archive { url, contents: contents.clone() }
			.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
			.await?;
		for item in contents {
			assert!(temp_dir.path().join(item).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn resolve_from_archive_is_noop() -> anyhow::Result<()> {
		let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz".to_string();
		let name = "polkadot".to_string();
		let contents =
			vec![name.clone(), "polkadot-execute-worker".into(), "polkadot-prepare-worker".into()];
		let temp_dir = tempdir()?;

		let source = Source::Archive { url, contents: contents.clone() };
		assert_eq!(
			source.clone().resolve(&name, None, temp_dir.path(), filters::polkadot).await,
			source
		);
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
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
		.await?;
		assert!(temp_dir.path().join(package).exists());
		Ok(())
	}

	#[tokio::test]
	async fn resolve_from_git_is_noop() -> anyhow::Result<()> {
		let url = Url::parse("https://github.com/hpaluch/rust-hello-world")?;
		let package = "hello_world".to_string();
		let temp_dir = tempdir()?;

		let source = Source::Git {
			url,
			reference: None,
			manifest: None,
			package: package.clone(),
			artifacts: vec![package.clone()],
		};
		assert_eq!(
			source
				.clone()
				.resolve(&package, None, temp_dir.path(), |f| filters::prefix(f, &package))
				.await,
			source
		);
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
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
		.await?;
		assert!(temp_dir.path().join(format!("{package}-{initial_commit}")).exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_github_release_archive_works() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let version = "stable2503";
		let tag_pattern = Some("polkadot-{version}".into());
		let fallback = "stable2412-4".into();
		let archive = format!("polkadot-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;

		Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: Some(format!("polkadot-{version}")),
			tag_pattern,
			prerelease: false,
			version_comparator,
			fallback,
			archive,
			contents: contents.map(|n| ArchiveFileSpec::new(n.into(), None, true)).to_vec(),
			latest: None,
		})
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
		.await?;
		for item in contents {
			assert!(temp_dir.path().join(format!("{item}-{version}")).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn resolve_from_github_release_archive_works() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let version = "stable2503";
		let tag_pattern = Some("polkadot-{version}".into());
		let fallback = "stable2412-4".into();
		let archive = format!("polkadot-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;

		// Determine release for comparison
		let mut releases: Vec<_> = crate::GitHub::new(owner.as_str(), repository.as_str())
			.releases(false)
			.await?
			.into_iter()
			.map(|r| r.tag_name)
			.collect();
		let sorted_releases = version_comparator(releases.as_mut_slice());

		let source = Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: None,
			tag_pattern,
			prerelease: false,
			version_comparator,
			fallback,
			archive,
			contents: contents.map(|n| ArchiveFileSpec::new(n.into(), None, true)).to_vec(),
			latest: None,
		});

		// Check results for a specified/unspecified version
		for version in [Some(version), None] {
			let source = source
				.clone()
				.resolve("polkadot", version, temp_dir.path(), filters::polkadot)
				.await;
			let expected_tag = version.map_or_else(
				|| sorted_releases.0.first().unwrap().into(),
				|v| format!("polkadot-{v}"),
			);
			let expected_latest = version.map_or_else(|| sorted_releases.0.first(), |_| None);
			assert!(matches!(
				source,
				Source::GitHub(ReleaseArchive { tag, latest, .. } )
					if tag == Some(expected_tag) && latest.as_ref() == expected_latest
			));
		}

		// Create a later version as a cached binary
		let cached_version = "polkadot-stable2612";
		File::create(temp_dir.path().join(cached_version))?;
		for version in [Some(version), None] {
			let source = source
				.clone()
				.resolve("polkadot", version, temp_dir.path(), filters::polkadot)
				.await;
			let expected_tag =
				version.map_or_else(|| cached_version.to_string(), |v| format!("polkadot-{v}"));
			let expected_latest =
				version.map_or_else(|| Some(cached_version.to_string()), |_| None);
			assert!(matches!(
				source,
				Source::GitHub(ReleaseArchive { tag, latest, .. } )
					if tag == Some(expected_tag) && latest == expected_latest
			));
		}

		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_github_release_archive_maps_contents() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let version = "stable2503";
		let tag_pattern = Some("polkadot-{version}".into());
		let name = "polkadot".to_string();
		let fallback = "stable2412-4".into();
		let archive = format!("{name}-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;
		let prefix = "test";

		Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: Some(format!("polkadot-{version}")),
			tag_pattern,
			prerelease: false,
			version_comparator,
			fallback,
			archive,
			contents: contents
				.map(|n| ArchiveFileSpec::new(n.into(), Some(format!("{prefix}-{n}").into()), true))
				.to_vec(),
			latest: None,
		})
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
		.await?;
		for item in contents {
			assert!(temp_dir.path().join(format!("{prefix}-{item}-{version}")).exists());
		}
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_latest_github_release_archive_works() -> anyhow::Result<()> {
		let owner = "r0gue-io".to_string();
		let repository = "polkadot".to_string();
		let tag_pattern = Some("polkadot-{version}".into());
		let name = "polkadot".to_string();
		let fallback = "stable2412-4".into();
		let archive = format!("{name}-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;

		Source::GitHub(ReleaseArchive {
			owner,
			repository,
			tag: None,
			tag_pattern,
			prerelease: false,
			version_comparator,
			fallback,
			archive,
			contents: contents.map(|n| ArchiveFileSpec::new(n.into(), None, true)).to_vec(),
			latest: None,
		})
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
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
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
		.await?;
		assert!(temp_dir.path().join(format!("{package}-{initial_commit}")).exists());
		Ok(())
	}

	#[tokio::test]
	async fn resolve_from_github_source_code_archive_is_noop() -> anyhow::Result<()> {
		let owner = "paritytech".to_string();
		let repository = "polkadot-sdk".to_string();
		let package = "polkadot".to_string();
		let temp_dir = tempdir()?;
		let initial_commit = "72dba98250a6267c61772cd55f8caf193141050f";
		let manifest = PathBuf::from("substrate/Cargo.toml");

		let source = Source::GitHub(SourceCodeArchive {
			owner,
			repository,
			reference: Some(initial_commit.to_string()),
			manifest: Some(manifest),
			package: package.clone(),
			artifacts: vec![package.clone()],
		});
		assert_eq!(
			source.clone().resolve(&package, None, temp_dir.path(), filters::polkadot).await,
			source
		);
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
		.source(temp_dir.path(), true, &Output, true, ArchiveType::Binary)
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
			.source(temp_dir.path(), false, &Output, true, ArchiveType::Binary)
			.await?;
		assert!(temp_dir.path().join(name).exists());
		Ok(())
	}

	#[tokio::test]
	async fn resolve_from_url_is_noop() -> anyhow::Result<()> {
		let url =
			"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc"
				.to_string();
		let name = "polkadot";
		let temp_dir = tempdir()?;

		let source = Source::Url { url, name: name.into() };
		assert_eq!(
			source.clone().resolve(name, None, temp_dir.path(), filters::polkadot).await,
			source
		);
		Ok(())
	}

	#[tokio::test]
	async fn from_archive_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz";
		let contents: Vec<_> = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"]
			.into_iter()
			.map(|b| ArchiveFileSpec::new(b.into(), Some(temp_dir.path().join(b)), true))
			.collect();

		from_archive(url, &contents, &Output, ArchiveType::Binary).await?;
		for ArchiveFileSpec { target, .. } in contents {
			assert!(target.unwrap().exists());
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
			package,
			&[(package, &path)],
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

		from_url(url, &path, &Output, ArchiveType::Binary).await?;
		assert!(path.exists());
		assert_ne!(metadata(path)?.permissions().mode() & 0o755, 0);
		Ok(())
	}

	#[tokio::test]
	async fn from_url_file_works() -> anyhow::Result<()> {
		let url =
			"https://github.com/paseo-network/runtimes/releases/download/v2.0.2/paseo-local.json";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join("paseo-local.json");

		from_url(url, &path, &Output, ArchiveType::File).await?;
		assert!(path.exists());
		// Files should not have executable permissions
		assert_eq!(metadata(&path)?.permissions().mode() & 0o111, 0);
		Ok(())
	}

	#[tokio::test]
	async fn from_archive_file_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let url =
			"https://github.com/paseo-network/runtimes/releases/download/v2.0.2/paseo-local.json";
		let contents: Vec<_> = vec![ArchiveFileSpec::new(
			"paseo-local.json".into(),
			Some(temp_dir.path().join("paseo-local")),
			true,
		)];

		from_archive(url, &contents, &Output, ArchiveType::File).await?;
		for ArchiveFileSpec { target, .. } in contents {
			let mut target_path = target.unwrap();
			// The extension is appended to the target path
			target_path.set_extension("json");
			assert!(target_path.exists());
			// Files should not have executable permissions
			assert_eq!(metadata(&target_path)?.permissions().mode() & 0o111, 0);
		}
		Ok(())
	}

	#[test]
	fn tag_pattern_works() {
		let pattern: TagPattern = "polkadot-{version}".into();
		assert_eq!(pattern.regex.as_str(), "^polkadot-(?P<version>.+)$");
		assert_eq!(pattern.pattern, "polkadot-{version}");
		assert_eq!(pattern, pattern.clone());

		for value in ["polkadot-stable2503", "stable2503"] {
			assert_eq!(pattern.resolve_tag(value).as_str(), "polkadot-stable2503");
		}
		assert_eq!(pattern.version("polkadot-stable2503"), Some("stable2503"));
	}

	fn version_comparator<T: AsRef<str> + Ord>(versions: &'_ mut [T]) -> SortedSlice<'_, T> {
		SortedSlice::by(versions, |a, b| parse_version(b.as_ref()).cmp(&parse_version(a.as_ref())))
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
	/// The source of a binary.
	pub trait Source {
		/// The type returned in the event of an error.
		type Error;

		/// Defines the source of a binary.
		fn source(&self) -> Result<super::Source, Self::Error>;
	}

	/// Traits for the sourcing of a binary using [strum]-based configuration.
	pub mod enums {
		use strum::EnumProperty;

		/// The source of a binary.
		pub trait Source {
			/// An error occurred on these trait calls.
			type Error;

			/// The name of the binary.
			fn binary(&self) -> Result<&'static str, Self::Error>;

			/// The name of the file
			fn file(&self) -> Result<&'static str, Self::Error>;

			/// The fallback version to be used when the latest version cannot be determined.
			fn fallback(&self) -> &str;

			/// Whether pre-releases are to be used.
			fn prerelease(&self) -> Option<bool>;
		}

		/// The source of a binary.
		pub trait Repository: Source {
			/// The repository to be used.
			fn repository(&self) -> &str;

			/// If applicable, a pattern to be used to determine applicable releases along with
			/// subcomponents from a release tag - e.g. `polkadot-{version}`.
			fn tag_pattern(&self) -> Option<&str>;
		}

		impl<T: EnumProperty + std::fmt::Debug> Source for T {
			type Error = crate::Error;

			fn binary(&self) -> Result<&'static str, Self::Error> {
				self.get_str("Binary").ok_or(Self::Error::StrumPropertyError(
					"Binary".to_owned(),
					format!("{:?}", self),
				))
			}

			fn file(&self) -> Result<&'static str, Self::Error> {
				self.get_str("File").ok_or(Self::Error::StrumPropertyError(
					"File".to_owned(),
					format!("{:?}", self),
				))
			}

			fn fallback(&self) -> &str {
				self.get_str("Fallback")
					.expect("expected specification of `Fallback` release tag")
			}

			fn prerelease(&self) -> Option<bool> {
				self.get_str("Prerelease").map(|v| {
					v.parse().expect("expected parachain prerelease value to be true/false")
				})
			}
		}

		impl<T: EnumProperty + std::fmt::Debug> Repository for T {
			fn repository(&self) -> &str {
				self.get_str("Repository").expect("expected specification of `Repository` url")
			}

			fn tag_pattern(&self) -> Option<&str> {
				self.get_str("TagPattern")
			}
		}
	}

	#[cfg(test)]
	mod tests {
		use super::enums::{Repository, Source};
		use strum_macros::{EnumProperty, VariantArray};

		#[derive(EnumProperty, VariantArray, Debug)]
		pub(super) enum Chain {
			#[strum(props(
				Repository = "https://github.com/paritytech/polkadot-sdk",
				Binary = "polkadot",
				Prerelease = "false",
				Fallback = "v1.12.0",
				TagPattern = "polkadot-{version}"
			))]
			Polkadot,
			#[strum(props(Repository = "https://github.com/r0gue-io/fallback", Fallback = "v1.0"))]
			Fallback,
		}

		#[test]
		fn binary_works() {
			assert_eq!("polkadot", Chain::Polkadot.binary().unwrap())
		}

		#[test]
		fn fallback_works() {
			assert_eq!("v1.12.0", Chain::Polkadot.fallback())
		}

		#[test]
		fn prerelease_works() {
			assert!(!Chain::Polkadot.prerelease().unwrap())
		}

		#[test]
		fn repository_works() {
			assert_eq!("https://github.com/paritytech/polkadot-sdk", Chain::Polkadot.repository())
		}

		#[test]
		fn tag_pattern_works() {
			assert_eq!("polkadot-{version}", Chain::Polkadot.tag_pattern().unwrap())
		}
	}
}

/// Filters which can be used when resolving a binary.
pub mod filters {
	/// A filter which ensures a candidate file name starts with a prefix.
	///
	/// # Arguments
	/// * `candidate` - the candidate to be evaluated.
	/// * `prefix` - the specified prefix.
	pub fn prefix(candidate: &str, prefix: &str) -> bool {
		candidate.starts_with(prefix) &&
			// Ignore any known related `polkadot`-prefixed binaries when `polkadot` only.
			(prefix != "polkadot" ||
				!["polkadot-execute-worker", "polkadot-prepare-worker", "polkadot-parachain"]
					.iter()
					.any(|i| candidate.starts_with(i)))
	}

	#[cfg(test)]
	pub(crate) fn polkadot(file: &str) -> bool {
		prefix(file, "polkadot")
	}
}
