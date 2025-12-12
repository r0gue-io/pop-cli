// SPDX-License-Identifier: GPL-3.0

use crate::{
	SortedSlice, Status,
	sourcing::{
		Error,
		GitHub::{ReleaseArchive, SourceCodeArchive},
		Source::{self, Archive, Git, GitHub},
		from_local_package,
	},
};
use std::path::{Path, PathBuf};

/// File extensions we allow in our sourcing
pub static ALLOWED_FILE_EXTENSIONS: [&str; 1] = [".json"];

/// The type of the Archive
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ArchiveType {
	/// If the archive is a binary
	Binary,
	/// If the archive is a file
	File,
}

/// A sourced archive.
#[derive(Debug, PartialEq)]
pub enum SourcedArchive {
	/// A local binary.
	Local {
		/// The name of the binary.
		name: String,
		/// The path of the binary.
		path: PathBuf,
		/// If applicable, the path to a manifest used to build the binary if missing.
		manifest: Option<PathBuf>,
		/// The archive type
		archive_type: ArchiveType,
	},
	/// A binary which needs to be sourced.
	Source {
		/// The name of the binary.
		name: String,
		/// The source of the binary.
		#[allow(private_interfaces)]
		source: Box<Source>,
		/// The cache to be used to store the binary.
		cache: PathBuf,
		/// The archive type
		archive_type: ArchiveType,
	},
}

impl SourcedArchive {
	/// Whether the archive exists.
	pub fn exists(&self) -> bool {
		self.path().exists()
	}

	/// If applicable, the latest version available.
	pub fn latest(&self) -> Option<&str> {
		match self {
			Self::Local { .. } => None,
			Self::Source { source, .. } => {
				if let GitHub(ReleaseArchive { latest, tag_pattern, .. }) = source.as_ref() {
					{
						// Extract the version from `latest`, provided it is a tag and that a tag
						// pattern exists
						latest.as_deref().and_then(|tag| {
							tag_pattern.as_ref().map_or(Some(tag), |pattern| pattern.version(tag))
						})
					}
				} else {
					None
				}
			},
		}
	}

	/// Whether the archive is defined locally.
	pub fn local(&self) -> bool {
		matches!(self, Self::Local { .. })
	}

	/// The name of the archive.
	pub fn name(&self) -> &str {
		match self {
			Self::Local { name, .. } => name,
			Self::Source { name, .. } => name,
		}
	}

	/// The archive type.
	pub fn archive_type(&self) -> ArchiveType {
		match *self {
			Self::Local { archive_type, .. } => archive_type,
			Self::Source { archive_type, .. } => archive_type,
		}
	}

	/// The path of the archive.
	pub fn path(&self) -> PathBuf {
		match self {
			Self::Local { path, .. } => path.to_path_buf(),
			Self::Source { name, cache, archive_type, .. } => {
				// Determine whether a specific version is specified
				self.version().map_or_else(
					|| cache.join(name),
					|v| match *archive_type {
						ArchiveType::File =>
							if let Some(ext_pos) = name.rfind('.') {
								cache.join(format!(
									"{}-{}{}",
									&name[..ext_pos],
									v,
									&name[ext_pos..]
								))
							} else {
								cache.join(format!("{name}-{v}"))
							},
						_ => cache.join(format!("{name}-{v}")),
					},
				)
			},
		}
	}

	/// Attempts to resolve a version of a archive based on whether one is specified, an existing
	/// version can be found cached locally, or uses the latest version.
	///
	/// # Arguments
	/// * `name` - The name of the archive.
	/// * `specified` - If available, a version explicitly specified.
	/// * `available` - The available versions, which are used to check for existing matches already
	///   cached locally or the latest otherwise.
	/// * `cache` - The location used for caching archives.
	pub(super) fn resolve_version<'a>(
		name: &str,
		specified: Option<&'a str>,
		available: &'a SortedSlice<impl AsRef<str>>,
		cache: &Path,
	) -> Option<&'a str> {
		match specified {
			Some(version) => Some(version),
			None => available
				.iter()
				// Default to latest version available locally
				.filter_map(|version| {
					let version = version.as_ref();
					let path = cache.join(format!("{name}-{version}"));
					path.exists().then_some(Some(version))
				})
				.nth(0)
				// Default to latest version
				.unwrap_or_else(|| available.first().map(|version| version.as_ref())),
		}
	}

	/// Sources the archive.
	///
	/// # Arguments
	/// * `release` - Whether any binary archives needing to be built should be done so using the
	///   release profile.
	/// * `status` - Used to observe status updates.
	/// * `verbose` - Whether verbose output is required.
	pub async fn source(
		&self,
		release: bool,
		status: &impl Status,
		verbose: bool,
	) -> Result<(), Error> {
		match self {
			Self::Local { name, path, manifest, .. } => match manifest {
				None => Err(Error::MissingArchive(format!(
					"The {path:?} binary cannot be sourced automatically."
				))),
				Some(manifest) =>
					from_local_package(manifest, name, release, status, verbose).await,
			},
			Self::Source { source, cache, archive_type, .. } =>
				source.source(cache, release, status, verbose, *archive_type).await,
		}
	}

	/// Whether any locally cached version can be replaced with a newer version.
	pub fn stale(&self) -> bool {
		// Only binaries sourced from GitHub release archives can currently be determined as stale
		let Self::Source { source, .. } = self else {
			return false;
		};

		let GitHub(ReleaseArchive { tag, latest, .. }) = source.as_ref() else {
			return false;
		};
		latest.as_ref().is_some_and(|l| tag.as_ref() != Some(l))
	}

	/// Specifies that the latest available versions are to be used (where possible).
	pub fn use_latest(&mut self) {
		let Self::Source { source, .. } = self else {
			return;
		};
		if let GitHub(ReleaseArchive { tag, latest: Some(latest), .. }) = source.as_mut() {
			*tag = Some(latest.clone())
		};
	}

	/// If applicable, the version of the binary.
	pub fn version(&self) -> Option<&str> {
		match self {
			Self::Local { .. } => None,
			Self::Source { source, .. } => match source.as_ref() {
				Git { reference, .. } => reference.as_ref().map(|r| r.as_str()),
				GitHub(source) => match source {
					ReleaseArchive { tag, tag_pattern, .. } => tag.as_ref().map(|tag| {
						// Use any tag pattern defined to extract a version, otherwise use the tag.
						tag_pattern.as_ref().and_then(|pattern| pattern.version(tag)).unwrap_or(tag)
					}),
					SourceCodeArchive { reference, .. } => reference.as_ref().map(|r| r.as_str()),
				},
				Archive { .. } | Source::Url { .. } => None,
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		polkadot_sdk::{sort_by_latest_semantic_version, sort_by_latest_version},
		sourcing::{ArchiveFileSpec, tests::Output},
		target,
	};
	use anyhow::Result;
	use duct::cmd;
	use std::{
		fs::{File, create_dir_all},
		os::unix::fs::PermissionsExt,
	};
	use tempfile::tempdir;
	use url::Url;

	#[test]
	fn local_binary_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);
		File::create(&path)?;

		let binary = SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest: None,
			archive_type: ArchiveType::Binary,
		};

		assert!(binary.exists());
		assert_eq!(binary.latest(), None);
		assert!(binary.local());
		assert_eq!(binary.name(), name);
		assert_eq!(binary.path(), path);
		assert!(!binary.stale());
		assert_eq!(binary.version(), None);
		Ok(())
	}

	#[test]
	fn local_package_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join("target/release").join(name);
		create_dir_all(path.parent().unwrap())?;
		File::create(&path)?;
		let manifest = Some(temp_dir.path().join("Cargo.toml"));

		let binary = SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest,
			archive_type: ArchiveType::Binary,
		};

		assert!(binary.exists());
		assert_eq!(binary.latest(), None);
		assert!(binary.local());
		assert_eq!(binary.name(), name);
		assert_eq!(binary.path(), path);
		assert!(!binary.stale());
		assert_eq!(binary.version(), None);
		Ok(())
	}

	#[test]
	fn resolve_version_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;

		let mut available = vec!["v1.13.0", "v1.12.0", "v1.11.0", "stable2409"];
		let available = sort_by_latest_version(available.as_mut_slice());

		// Specified
		let specified = Some("v1.12.0");
		assert_eq!(
			SourcedArchive::resolve_version(name, specified, &available, temp_dir.path()),
			specified
		);
		// Latest
		assert_eq!(
			SourcedArchive::resolve_version(name, None, &available, temp_dir.path()).unwrap(),
			"stable2409"
		);
		// Cached
		File::create(temp_dir.path().join(format!("{name}-{}", available[1])))?;
		assert_eq!(
			SourcedArchive::resolve_version(name, None, &available, temp_dir.path()).unwrap(),
			available[1]
		);
		Ok(())
	}

	#[test]
	fn sourced_from_archive_works() -> Result<()> {
		let name = "polkadot";
		let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz".to_string();
		let contents = vec![
			name.to_string(),
			"polkadot-execute-worker".into(),
			"polkadot-prepare-worker".into(),
		];
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);
		File::create(&path)?;

		let mut binary = SourcedArchive::Source {
			name: name.to_string(),
			source: Archive { url: url.to_string(), contents }.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::Binary,
		};

		assert!(binary.exists());
		assert_eq!(binary.latest(), None);
		assert!(!binary.local());
		assert_eq!(binary.name(), name);
		assert_eq!(binary.path(), path);
		assert!(!binary.stale());
		assert_eq!(binary.version(), None);
		binary.use_latest();
		assert_eq!(binary.version(), None);
		assert_eq!(binary.archive_type(), ArchiveType::Binary);
		Ok(())
	}

	#[test]
	fn sourced_from_git_works() -> Result<()> {
		let package = "hello_world";
		let url = Url::parse("https://github.com/hpaluch/rust-hello-world")?;
		let temp_dir = tempdir()?;
		for reference in [None, Some("436b7dbffdfaaf7ad90bf44ae8fdcb17eeee65a3".to_string())] {
			let path = temp_dir.path().join(
				reference
					.as_ref()
					.map_or(package.into(), |reference| format!("{package}-{reference}")),
			);
			File::create(&path)?;

			let mut binary = SourcedArchive::Source {
				name: package.to_string(),
				source: Git {
					url: url.clone(),
					reference: reference.clone(),
					manifest: None,
					package: package.to_string(),
					artifacts: vec![package.to_string()],
				}
				.into(),
				cache: temp_dir.path().to_path_buf(),
				archive_type: ArchiveType::Binary,
			};

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(!binary.local());
			assert_eq!(binary.name(), package);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), reference.as_deref());
			binary.use_latest();
			assert_eq!(binary.version(), reference.as_deref());
			assert_eq!(binary.archive_type(), ArchiveType::Binary);
		}

		Ok(())
	}

	#[test]
	fn sourced_from_github_release_archive_works() -> Result<()> {
		let owner = "r0gue-io";
		let repository = "polkadot";
		let tag_pattern = "polkadot-{version}";
		let name = "polkadot";
		let archive = format!("{name}-{}.tar.gz", target()?);
		let fallback = "stable2412-4".to_string();
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;
		for tag in [None, Some("stable2412".to_string())] {
			let path = temp_dir
				.path()
				.join(tag.as_ref().map_or(name.to_string(), |t| format!("{name}-{t}")));
			File::create(&path)?;
			for latest in [None, Some("polkadot-stable2503".to_string())] {
				let mut binary = SourcedArchive::Source {
					name: name.to_string(),
					source: GitHub(ReleaseArchive {
						owner: owner.into(),
						repository: repository.into(),
						tag: tag.clone(),
						tag_pattern: Some(tag_pattern.into()),
						prerelease: false,
						version_comparator: sort_by_latest_semantic_version,
						fallback: fallback.clone(),
						archive: archive.clone(),
						contents: contents
							.into_iter()
							.map(|b| ArchiveFileSpec::new(b.into(), None, true))
							.collect(),
						latest: latest.clone(),
					})
					.into(),
					cache: temp_dir.path().to_path_buf(),
					archive_type: ArchiveType::Binary,
				};

				let latest = latest.as_ref().map(|l| l.replace("polkadot-", ""));

				assert!(binary.exists());
				assert_eq!(binary.latest(), latest.as_deref());
				assert!(!binary.local());
				assert_eq!(binary.name(), name);
				assert_eq!(binary.path(), path);
				assert_eq!(binary.stale(), latest.is_some());
				assert_eq!(binary.version(), tag.as_deref());
				binary.use_latest();
				if latest.is_some() {
					assert_eq!(binary.version(), latest.as_deref());
				}
				assert_eq!(binary.archive_type(), ArchiveType::Binary);
			}
		}
		Ok(())
	}

	#[test]
	fn sourced_from_github_source_code_archive_works() -> Result<()> {
		let owner = "paritytech";
		let repository = "polkadot-sdk";
		let package = "polkadot";
		let manifest = "substrate/Cargo.toml";
		let temp_dir = tempdir()?;
		for reference in [None, Some("72dba98250a6267c61772cd55f8caf193141050f".to_string())] {
			let path = temp_dir
				.path()
				.join(reference.as_ref().map_or(package.to_string(), |t| format!("{package}-{t}")));
			File::create(&path)?;
			let mut binary = SourcedArchive::Source {
				name: package.to_string(),
				source: GitHub(SourceCodeArchive {
					owner: owner.to_string(),
					repository: repository.to_string(),
					reference: reference.clone(),
					manifest: Some(PathBuf::from(manifest)),
					package: package.to_string(),
					artifacts: vec![package.to_string()],
				})
				.into(),
				cache: temp_dir.path().to_path_buf(),
				archive_type: ArchiveType::Binary,
			};

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(!binary.local());
			assert_eq!(binary.name(), package);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), reference.as_deref());
			binary.use_latest();
			assert_eq!(binary.version(), reference.as_deref());
			assert_eq!(binary.archive_type(), ArchiveType::Binary);
		}
		Ok(())
	}

	#[test]
	fn sourced_from_url_works() -> Result<()> {
		let name = "polkadot";
		let url =
			"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);
		File::create(&path)?;

		let mut binary = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url { url: url.to_string(), name: name.to_string() }.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::Binary,
		};

		assert!(binary.exists());
		assert_eq!(binary.latest(), None);
		assert!(!binary.local());
		assert_eq!(binary.name(), name);
		assert_eq!(binary.path(), path);
		assert!(!binary.stale());
		assert_eq!(binary.version(), None);
		binary.use_latest();
		assert_eq!(binary.version(), None);
		assert_eq!(binary.archive_type(), ArchiveType::Binary);
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_local_binary_not_supported() -> Result<()> {
		let name = "polkadot".to_string();
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(&name);
		assert!(matches!(
			SourcedArchive::Local { name, path: path.clone(), manifest: None, archive_type: ArchiveType::Binary }.source(true, &Output, true).await,
			Err(Error::MissingArchive(error)) if error == format!("The {path:?} binary cannot be sourced automatically.")
		));
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_local_package_works() -> Result<()> {
		let temp_dir = tempdir()?;
		let name = "hello_world";
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let path = temp_dir.path().join(name);
		let manifest = Some(path.join("Cargo.toml"));
		let path = path.join("target/release").join(name);
		SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest,
			archive_type: ArchiveType::Binary,
		}
		.source(true, &Output, true)
		.await?;
		assert!(path.exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_binary_from_url_works() -> Result<()> {
		let name = "polkadot";
		let url =
			"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);

		SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url { url: url.to_string(), name: name.to_string() }.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::Binary,
		}
		.source(true, &Output, true)
		.await?;
		assert!(path.exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_file_from_url_works() -> Result<()> {
		let name = "paseo-local.json";
		let url =
			"https://github.com/paseo-network/runtimes/releases/download/v2.0.2/paseo-local.json";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);

		SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url { url: url.to_string(), name: name.to_string() }.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::File,
		}
		.source(true, &Output, true)
		.await?;
		assert!(path.exists());
		// Files should not have executable permissions
		assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o111, 0);
		Ok(())
	}

	#[test]
	fn file_archive_path_with_version_works() -> Result<()> {
		let name = "paseo-local.json";
		let temp_dir = tempdir()?;
		let version = "v2.0.2";

		// Create a versioned file
		let expected_path = temp_dir.path().join(format!("paseo-local-{}.json", version));
		File::create(&expected_path)?;

		let binary = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::GitHub(crate::sourcing::GitHub::ReleaseArchive {
				owner: "paseo-network".to_string(),
				repository: "runtimes".to_string(),
				tag: Some(version.to_string()),
				tag_pattern: None,
				prerelease: false,
				version_comparator: crate::polkadot_sdk::sort_by_latest_semantic_version,
				fallback: "v2.0.2".to_string(),
				archive: format!("{}.json", name),
				contents: vec![],
				latest: None,
			})
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::File,
		};

		assert_eq!(binary.path(), expected_path);
		assert!(binary.exists());
		Ok(())
	}

	#[test]
	fn archive_type_binary_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);
		File::create(&path)?;

		// Test Local Binary
		let local_binary = SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest: None,
			archive_type: ArchiveType::Binary,
		};
		assert_eq!(local_binary.archive_type(), ArchiveType::Binary);

		// Test Source Binary
		let source_binary = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url {
				url: "https://example.com/polkadot".to_string(),
				name: name.to_string(),
			}
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::Binary,
		};
		assert_eq!(source_binary.archive_type(), ArchiveType::Binary);
		Ok(())
	}

	#[test]
	fn archive_type_file_works() -> Result<()> {
		let name = "chain-spec.json";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);
		File::create(&path)?;

		// Test Local File
		let local_file = SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest: None,
			archive_type: ArchiveType::File,
		};
		assert_eq!(local_file.archive_type(), ArchiveType::File);

		// Test Source File
		let source_file = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url {
				url: "https://example.com/chain-spec.json".to_string(),
				name: name.to_string(),
			}
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::File,
		};
		assert_eq!(source_file.archive_type(), ArchiveType::File);
		Ok(())
	}

	#[test]
	fn path_for_binary_without_version_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;

		let binary = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url {
				url: "https://example.com/polkadot".to_string(),
				name: name.to_string(),
			}
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::Binary,
		};

		// Without version, path should be cache/name
		assert_eq!(binary.path(), temp_dir.path().join(name));
		Ok(())
	}

	#[test]
	fn path_for_binary_with_version_works() -> Result<()> {
		let name = "polkadot";
		let version = "v1.0.0";
		let temp_dir = tempdir()?;

		let binary = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::GitHub(crate::sourcing::GitHub::ReleaseArchive {
				owner: "paritytech".to_string(),
				repository: "polkadot-sdk".to_string(),
				tag: Some(version.to_string()),
				tag_pattern: None,
				prerelease: false,
				version_comparator: crate::polkadot_sdk::sort_by_latest_semantic_version,
				fallback: "v1.0.0".to_string(),
				archive: "polkadot.tar.gz".to_string(),
				contents: vec![],
				latest: None,
			})
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::Binary,
		};

		// With version, path should be cache/name-version
		assert_eq!(binary.path(), temp_dir.path().join(format!("{}-{}", name, version)));
		Ok(())
	}

	#[test]
	fn path_for_file_without_version_works() -> Result<()> {
		let name = "chain-spec.json";
		let temp_dir = tempdir()?;

		let file = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::Url {
				url: "https://example.com/chain-spec.json".to_string(),
				name: name.to_string(),
			}
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::File,
		};

		// Without version, path should be cache/name
		assert_eq!(file.path(), temp_dir.path().join(name));
		Ok(())
	}

	#[test]
	fn path_for_file_with_version_and_extension_works() -> Result<()> {
		let name = "paseo-local.json";
		let version = "v2.0.2";
		let temp_dir = tempdir()?;

		let file = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::GitHub(crate::sourcing::GitHub::ReleaseArchive {
				owner: "paseo-network".to_string(),
				repository: "runtimes".to_string(),
				tag: Some(version.to_string()),
				tag_pattern: None,
				prerelease: false,
				version_comparator: crate::polkadot_sdk::sort_by_latest_semantic_version,
				fallback: "v2.0.2".to_string(),
				archive: "paseo-local.json".to_string(),
				contents: vec![],
				latest: None,
			})
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::File,
		};

		// With version and extension, path should be cache/name-version.ext
		// e.g., paseo-local-v2.0.2.json
		assert_eq!(file.path(), temp_dir.path().join(format!("paseo-local-{}.json", version)));
		Ok(())
	}

	#[test]
	fn path_for_file_with_version_no_extension_works() -> Result<()> {
		let name = "chain-spec";
		let version = "v1.0.0";
		let temp_dir = tempdir()?;

		let file = SourcedArchive::Source {
			name: name.to_string(),
			source: Source::GitHub(crate::sourcing::GitHub::ReleaseArchive {
				owner: "example".to_string(),
				repository: "repo".to_string(),
				tag: Some(version.to_string()),
				tag_pattern: None,
				prerelease: false,
				version_comparator: crate::polkadot_sdk::sort_by_latest_semantic_version,
				fallback: "v1.0.0".to_string(),
				archive: "chain-spec".to_string(),
				contents: vec![],
				latest: None,
			})
			.into(),
			cache: temp_dir.path().to_path_buf(),
			archive_type: ArchiveType::File,
		};

		// With version but no extension, path should be cache/name-version
		assert_eq!(file.path(), temp_dir.path().join(format!("{}-{}", name, version)));
		Ok(())
	}

	#[test]
	fn path_for_local_binary_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join("custom/path").join(name);

		let binary = SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest: None,
			archive_type: ArchiveType::Binary,
		};

		// Local binaries should return their exact path
		assert_eq!(binary.path(), path);
		Ok(())
	}

	#[test]
	fn path_for_local_file_works() -> Result<()> {
		let name = "chain-spec.json";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join("custom/path").join(name);

		let file = SourcedArchive::Local {
			name: name.to_string(),
			path: path.clone(),
			manifest: None,
			archive_type: ArchiveType::File,
		};

		// Local files should return their exact path
		assert_eq!(file.path(), path);
		Ok(())
	}
}
