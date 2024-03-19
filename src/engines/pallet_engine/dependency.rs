//! Dependency representations for Pallets
use strum_macros::{Display, EnumString};

#[derive(EnumString, Display, Debug)]
pub(super) enum Features {
	#[strum(serialize = "std")]
	Std,
	#[strum(serialize = "runtime-benchmarks")]
	RuntimeBenchmarks,
	#[strum(serialize = "try-runtime")]
	TryRuntime,
	/// Custom feature
	Custom(String),
}
#[derive(Display, Debug)]
pub(super) enum Location {
	/// Local path is form `path = "X"`
	Local(std::path::PathBuf),
	/// `git = {url}`
	Git(reqwest::Url),
	/// String should be of format `version = "X"`
	CratesIO(semver::Version),
}
impl From<reqwest::Url> for Location {
	fn from(url: reqwest::Url) -> Self {
		Self::Git(url)
	}
}
impl From<std::path::PathBuf> for Location {
	fn from(path: std::path::PathBuf) -> Self {
		Self::Local(path)
	}
}
impl<'a> From<&'a std::path::Path> for Location {
	fn from(path: &'a std::path::Path) -> Self {
		Self::Local(path.to_path_buf())
	}
}
impl From<semver::Version> for Location {
	fn from(version: semver::Version) -> Self {
		Self::CratesIO(version)
	}
}
impl Into<String> for Location {
	fn into(self) -> String {
		match self {
			Location::Local(path) => format!("path = \"{}\"", path.display()),
			Location::Git(url) => format!("git = \"{}\"", url),
			Location::CratesIO(version) => format!("version = \"{}\"", version),
		}
	}
}
impl Into<toml_edit::Value> for Location {
	fn into(self) -> toml_edit::Value {
		Into::<String>::into(self).into()
	}
}

#[derive(Debug)]
pub(super) struct Dependency {
	pub(super) features: Vec<Features>,
	/// Maybe local path, git url, or from crates.io in which case we will use this for version
	pub(super) path: Location,
	pub(super) default_features: bool,
}

impl Dependency {
	/// Create dependencies required for adding a local pallet-parachain-template to runtime
	/// ..$(runtime)/pallets/pallet-parachain-template
	pub(super) fn local_template_runtime() -> Self {
		Self {
			features: vec![Features::RuntimeBenchmarks, Features::TryRuntime, Features::Std],
			// TODO hardcode for now
			path: std::path::Path::new("../pallets/pallet-parachain-template")
				.to_path_buf()
				.into(),
			default_features: false,
		}
	}
	// TODO: Remove code - Node doesn't require template pallet deps by default
	// but this maybe desirable for custom pallets.
	// /// Create dependencies required for adding a pallet-parachain-template to node
	// pub(super) fn template_node() -> Self {
	// 	Self {
	// 		features: vec![Features::RuntimeBenchmarks, Features::TryRuntime],
	// 		// TODO hardcode for now
	// 		path: format!("../pallets/pallet-parachain-template"),
	// 		default_features: true,
	// 	}
	// }
}
