//! Dependency representations for Pallets
use std::path;
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
#[derive(Display, Debug, Clone)]
pub(super) enum Location {
	/// Local path is form `path = "X"`
	Local(std::path::PathBuf, Option<semver::Version>),
	/// `git = {url}`
	Git(reqwest::Url, Option<semver::Version>),
	/// String should be of format `version = "X"`
	CratesIO(semver::Version),
}
impl From<reqwest::Url> for Location {
	fn from(url: reqwest::Url) -> Self {
		Self::Git(url, None)
	}
}
impl From<std::path::PathBuf> for Location {
	fn from(path: std::path::PathBuf) -> Self {
		Self::Local(path, None)
	}
}
impl From<(std::path::PathBuf, semver::Version)> for Location {
	fn from(info: (std::path::PathBuf, semver::Version)) -> Self {
		Self::Local(info.0, Some(info.1))
	}
}
impl<'a> From<&'a std::path::Path> for Location {
	fn from(path: &'a std::path::Path) -> Self {
		Self::Local(path.to_path_buf(), None)
	}
}
impl<'a> From<(&'a std::path::Path, semver::Version)> for Location {
	fn from(info: (&'a std::path::Path, semver::Version)) -> Self {
		Self::Local(info.0.to_path_buf(), Some(info.1.into()))
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
			Location::Local(path, Some(version)) => {
				format!("{{ path = \"{}\", version = \"{}\" }}", path.display(), version)
			},
			Location::Local(path, _) => format!("{{ path = \"{}\" }}", path.display()),
			Location::Git(url, Some(version)) => {
				format!("{{ git = \"{}\", version = \"{}\" }}", url, version)
			},
			Location::Git(url, _) => format!("{{ git = \"{}\" }}", url),
			Location::CratesIO(version) => format!("{{ version = \"{}\" }}", version),
		}
	}
}
impl Into<toml_edit::Value> for Location {
	fn into(self) -> toml_edit::Value {
		let s = Into::<String>::into(self);
		let t = s
			.parse::<toml_edit::Value>()
			.expect("Location String parse as Toml Value failed");
		toml_edit::Value::InlineTable(
			t.as_inline_table().expect(" Parsed Location -> ILT cast infallible").to_owned(),
		)
	}
}

#[derive(Debug)]
pub(super) struct Dependency {
	/// Name for the dependency
	pub(super) name: String,
	/// Additional features that need to be enabled. Format -> {name}/{feature}
	pub(super) features: Vec<Features>,
	/// Maybe local path, git url, or from crates.io in which case we will use this for version
	pub(super) path: Location,
	/// Default features such as `std` are disabled by default for runtime pallet dependencies
	pub(super) default_features: bool,
}

impl Dependency {
	/// Generate the main dependency as an inline table
	pub(super) fn entry(&self) -> toml_edit::InlineTable {
		let mut t = toml_edit::Table::new();
		let location = Into::<toml_edit::Value>::into(self.path.clone());
		t.extend(
			location
				.as_inline_table()
				.expect("Location to String should produce valid inline table")
				.to_owned(),
		);
		t["default-features"] = toml_edit::value(self.default_features);
		t.into_inline_table()
	}
	/// Create dependencies required for adding a local pallet-parachain-template to runtime
	/// ..$(runtime)/pallets/pallet-parachain-template
	pub(super) fn local_template_runtime() -> Self {
		Self {
			name: format!("pallet-parachain-template"),
			features: vec![Features::RuntimeBenchmarks, Features::TryRuntime, Features::Std],
			path: (
				// TODO hardcode for now
				// The reason is, `pop new pallet` places a new pallet on $(workspace_root)/pallets
				std::path::Path::new("../pallets/pallet-parachain-template").to_path_buf(),
				semver::Version::new(1, 0, 0),
			).into(),
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
