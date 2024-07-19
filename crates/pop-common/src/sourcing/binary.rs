use crate::sourcing::{
	errors::Error,
	sourcing::{
		from_local_package, GitHub::ReleaseArchive, GitHub::SourceCodeArchive, Source,
		Source::Archive, Source::Git, Source::GitHub,
	},
};
use std::path::{Path, PathBuf};
use url::Url;

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
			Self::Source { source, .. } => {
				if let GitHub(ReleaseArchive { latest, .. }) = source {
					latest.as_deref()
				} else {
					None
				}
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

	/// Attempts to resolve a version of a binary based on whether one is specified, an existing version
	/// can be found cached locally, or uses the latest version.
	///
	/// # Arguments
	/// * `name` - The name of the binary.
	/// * `specified` - If available, a version explicitly specified.
	/// * `available` - The available versions, used to check for those cached locally or the latest otherwise.
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
					available.get(0).and_then(|version| Some(version.as_ref().to_string())),
				),
		}
	}

	/// Sources the binary.
	///
	/// # Arguments
	/// * `release` - Whether any binaries needing to be built should be done so using the release profile.
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
				None => {
					return Err(Error::MissingBinary(format!(
						"The {path:?} binary cannot be sourced automatically."
					)))
				},
				Some(manifest) => {
					from_local_package(manifest, name, release, status, verbose).await
				},
			},
			Self::Source { source, cache, .. } => {
				source.source(cache, release, status, verbose).await
			},
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
		if let Self::Source { source: GitHub(ReleaseArchive { tag, latest, .. }), .. } = self {
			if let Some(latest) = latest {
				*tag = Some(latest.clone())
			}
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

/// Trait for observing status updates.
pub trait Status {
	/// Update the observer with the provided `status`.
	fn update(&self, status: &str);
}

impl Status for () {
	// no-op: status updates are ignored
	fn update(&self, _: &str) {}
}

/// Determines the target triple based on the current platform.
pub fn target() -> Result<&'static str, Error> {
	use std::env::consts::*;

	if OS == "windows" {
		return Err(Error::UnsupportedPlatform { arch: ARCH, os: OS });
	}

	match ARCH {
		"aarch64" => {
			return match OS {
				"macos" => Ok("aarch64-apple-darwin"),
				_ => Ok("aarch64-unknown-linux-gnu"),
			}
		},
		"x86_64" | "x86" => {
			return match OS {
				"macos" => Ok("x86_64-apple-darwin"),
				_ => Ok("x86_64-unknown-linux-gnu"),
			}
		},
		&_ => {},
	}
	Err(Error::UnsupportedPlatform { arch: ARCH, os: OS })
}
