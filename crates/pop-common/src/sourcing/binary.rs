// SPDX-License-Identifier: GPL-3.0

use crate::{
	sourcing::{
		from_local_package, Error,
		GitHub::{ReleaseArchive, SourceCodeArchive},
		Source,
		Source::{Archive, Git, GitHub},
	},
	Status,
};
use std::path::{Path, PathBuf};

/// A binary used to launch a node.
#[derive(Debug, PartialEq)]
pub enum Binary {
	/// A local binary.
	Local {
		/// The name of the binary.
		name: String,
		/// The path of the binary.
		path: PathBuf,
		/// If applicable, the path to a manifest used to build the binary if missing.
		manifest: Option<PathBuf>,
	},
	/// A binary which needs to be sourced.
	Source {
		/// The name of the binary.
		name: String,
		/// The source of the binary.
		#[allow(private_interfaces)]
		source: Source,
		/// The cache to be used to store the binary.
		cache: PathBuf,
	},
}

impl Binary {
	/// Whether the binary exists.
	pub fn exists(&self) -> bool {
		self.path().exists()
	}

	/// If applicable, the latest version available.
	pub fn latest(&self) -> Option<&str> {
		match self {
			Self::Local { .. } => None,
			Self::Source { source, .. } =>
				if let GitHub(ReleaseArchive { latest, .. }) = source {
					latest.as_deref()
				} else {
					None
				},
		}
	}

	/// Whether the binary is defined locally.
	pub fn local(&self) -> bool {
		matches!(self, Self::Local { .. })
	}

	/// The name of the binary.
	pub fn name(&self) -> &str {
		match self {
			Self::Local { name, .. } => name,
			Self::Source { name, .. } => name,
		}
	}

	/// The path of the binary.
	pub fn path(&self) -> PathBuf {
		match self {
			Self::Local { path, .. } => path.to_path_buf(),
			Self::Source { name, source, cache, .. } => {
				// Determine whether a specific version is specified
				let version = match source {
					Git { reference, .. } => reference.as_ref(),
					GitHub(source) => match source {
						ReleaseArchive { tag, .. } => tag.as_ref(),
						SourceCodeArchive { reference, .. } => reference.as_ref(),
					},
					Archive { .. } | Source::Url { .. } => None,
				};
				version.map_or_else(|| cache.join(name), |v| cache.join(format!("{name}-{v}")))
			},
		}
	}

	/// Attempts to resolve a version of a binary based on whether one is specified, an existing
	/// version can be found cached locally, or uses the latest version.
	///
	/// # Arguments
	/// * `name` - The name of the binary.
	/// * `specified` - If available, a version explicitly specified.
	/// * `available` - The available versions, used to check for those cached locally or the latest
	///   otherwise.
	/// * `cache` - The location used for caching binaries.
	pub fn resolve_version(
		name: &str,
		specified: Option<&str>,
		available: &[impl AsRef<str>],
		cache: &Path,
	) -> Option<String> {
		match specified {
			Some(version) => Some(version.to_string()),
			None => available
				.iter()
				.map(|v| v.as_ref())
				// Default to latest version available locally
				.filter_map(|version| {
					let path = cache.join(format!("{name}-{version}"));
					path.exists().then_some(Some(version.to_string()))
				})
				.nth(0)
				.unwrap_or(
					// Default to latest version
					available.first().map(|version| version.as_ref().to_string()),
				),
		}
	}

	/// Sources the binary.
	///
	/// # Arguments
	/// * `release` - Whether any binaries needing to be built should be done so using the release
	///   profile.
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
				None => Err(Error::MissingBinary(format!(
					"The {path:?} binary cannot be sourced automatically."
				))),
				Some(manifest) =>
					from_local_package(manifest, name, release, status, verbose).await,
			},
			Self::Source { source, cache, .. } =>
				source.source(cache, release, status, verbose).await,
		}
	}

	/// Whether any locally cached version can be replaced with a newer version.
	pub fn stale(&self) -> bool {
		// Only binaries sourced from GitHub release archives can currently be determined as stale
		let Self::Source { source: GitHub(ReleaseArchive { tag, latest, .. }), .. } = self else {
			return false;
		};
		latest.as_ref().map_or(false, |l| tag.as_ref() != Some(l))
	}

	/// Specifies that the latest available versions are to be used (where possible).
	pub fn use_latest(&mut self) {
		if let Self::Source {
			source: GitHub(ReleaseArchive { tag, latest: Some(latest), .. }),
			..
		} = self
		{
			*tag = Some(latest.clone())
		};
	}

	/// If applicable, the version of the binary.
	pub fn version(&self) -> Option<&str> {
		match self {
			Self::Local { .. } => None,
			Self::Source { source, .. } => match source {
				Git { reference, .. } => reference.as_ref(),
				GitHub(source) => match source {
					ReleaseArchive { tag, .. } => tag.as_ref(),
					SourceCodeArchive { reference, .. } => reference.as_ref(),
				},
				Archive { .. } | Source::Url { .. } => None,
			},
		}
		.map(|r| r.as_str())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{sourcing::tests::Output, target};
	use anyhow::Result;
	use duct::cmd;
	use std::fs::{create_dir_all, File};
	use tempfile::tempdir;
	use url::Url;

	#[test]
	fn local_binary_works() -> Result<()> {
		let name = "polkadot";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);
		File::create(&path)?;

		let binary = Binary::Local { name: name.to_string(), path: path.clone(), manifest: None };

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
		create_dir_all(&path.parent().unwrap())?;
		File::create(&path)?;
		let manifest = Some(temp_dir.path().join("Cargo.toml"));

		let binary = Binary::Local { name: name.to_string(), path: path.clone(), manifest };

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

		let available = vec!["v1.13.0", "v1.12.0", "v1.11.0"];

		// Specified
		let specified = Some("v1.12.0");
		assert_eq!(
			Binary::resolve_version(name, specified, &available, temp_dir.path()).unwrap(),
			specified.unwrap()
		);
		// Latest
		assert_eq!(
			Binary::resolve_version(name, None, &available, temp_dir.path()).unwrap(),
			available[0]
		);
		// Cached
		File::create(temp_dir.path().join(format!("{name}-{}", available[1])))?;
		assert_eq!(
			Binary::resolve_version(name, None, &available, temp_dir.path()).unwrap(),
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

		let mut binary = Binary::Source {
			name: name.to_string(),
			source: Archive { url: url.to_string(), contents },
			cache: temp_dir.path().to_path_buf(),
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

			let mut binary = Binary::Source {
				name: package.to_string(),
				source: Git {
					url: url.clone(),
					reference: reference.clone(),
					manifest: None,
					package: package.to_string(),
					artifacts: vec![package.to_string()],
				},
				cache: temp_dir.path().to_path_buf(),
			};

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(!binary.local());
			assert_eq!(binary.name(), package);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), reference.as_ref().map(|r| r.as_str()));
			binary.use_latest();
			assert_eq!(binary.version(), reference.as_ref().map(|r| r.as_str()));
		}

		Ok(())
	}

	#[test]
	fn sourced_from_github_release_archive_works() -> Result<()> {
		let owner = "r0gue-io";
		let repository = "polkadot";
		let tag_format = "polkadot-{tag}";
		let name = "polkadot";
		let archive = format!("{name}-{}.tar.gz", target()?);
		let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
		let temp_dir = tempdir()?;
		for tag in [None, Some("v1.12.0".to_string())] {
			let path = temp_dir
				.path()
				.join(tag.as_ref().map_or(name.to_string(), |t| format!("{name}-{t}")));
			File::create(&path)?;
			for latest in [None, Some("v2.0.0".to_string())] {
				let mut binary = Binary::Source {
					name: name.to_string(),
					source: GitHub(ReleaseArchive {
						owner: owner.into(),
						repository: repository.into(),
						tag: tag.clone(),
						tag_format: Some(tag_format.to_string()),
						archive: archive.clone(),
						contents: contents.into_iter().map(|b| (b, None)).collect(),
						latest: latest.clone(),
					}),
					cache: temp_dir.path().to_path_buf(),
				};

				assert!(binary.exists());
				assert_eq!(binary.latest(), latest.as_ref().map(|l| l.as_str()));
				assert!(!binary.local());
				assert_eq!(binary.name(), name);
				assert_eq!(binary.path(), path);
				assert_eq!(binary.stale(), latest.is_some());
				assert_eq!(binary.version(), tag.as_ref().map(|t| t.as_str()));
				binary.use_latest();
				if latest.is_some() {
					assert_eq!(binary.version(), latest.as_ref().map(|l| l.as_str()));
				}
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
			let mut binary = Binary::Source {
				name: package.to_string(),
				source: GitHub(SourceCodeArchive {
					owner: owner.to_string(),
					repository: repository.to_string(),
					reference: reference.clone(),
					manifest: Some(PathBuf::from(manifest)),
					package: package.to_string(),
					artifacts: vec![package.to_string()],
				}),
				cache: temp_dir.path().to_path_buf(),
			};

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(!binary.local());
			assert_eq!(binary.name(), package);
			assert_eq!(binary.path(), path);
			assert_eq!(binary.stale(), false);
			assert_eq!(binary.version(), reference.as_ref().map(|r| r.as_str()));
			binary.use_latest();
			assert_eq!(binary.version(), reference.as_ref().map(|l| l.as_str()));
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

		let mut binary = Binary::Source {
			name: name.to_string(),
			source: Source::Url { url: url.to_string(), name: name.to_string() },
			cache: temp_dir.path().to_path_buf(),
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
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_local_binary_not_supported() -> Result<()> {
		let name = "polkadot".to_string();
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(&name);
		assert!(matches!(
			Binary::Local { name, path: path.clone(), manifest: None }.source(true, &Output, true).await,
			Err(Error::MissingBinary(error)) if error == format!("The {path:?} binary cannot be sourced automatically.")
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
		Binary::Local { name: name.to_string(), path: path.clone(), manifest }
			.source(true, &Output, true)
			.await?;
		assert!(path.exists());
		Ok(())
	}

	#[tokio::test]
	async fn sourcing_from_url_works() -> Result<()> {
		let name = "polkadot";
		let url =
			"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc";
		let temp_dir = tempdir()?;
		let path = temp_dir.path().join(name);

		Binary::Source {
			name: name.to_string(),
			source: Source::Url { url: url.to_string(), name: name.to_string() },
			cache: temp_dir.path().to_path_buf(),
		}
		.source(true, &Output, true)
		.await?;
		assert!(path.exists());
		Ok(())
	}
}
