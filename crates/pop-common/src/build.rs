use std::{fmt, path::{Path, PathBuf}};
use strum_macros::{VariantArray, AsRefStr, EnumMessage, EnumString};

/// Enum representing a build profile.
#[derive(
	AsRefStr,
	Clone,
	Default,
	Debug,
	EnumString,
	EnumMessage,
	VariantArray,
	Eq,
	PartialEq,
)]
pub enum Profile {
	/// Debug profile, optimized for debugging.
	#[strum(
		serialize = "debug",
		message = "Debug",
		detailed_message = "Optimized for debugging."
	)]
	Debug,
	/// Release profile, optimized without any debugging functionality.
	#[default]
	#[strum(
		serialize = "release",
		message = "Release",
		detailed_message = "Optimized without any debugging functionality."
	)]
	Release,
	/// Production profile, optimized for ultimate performance.
	#[strum(
		serialize = "production",
		message = "Production",
		detailed_message = "Optimized for ultimate performance."
	)]
	Production,
}

impl Profile {
	/// Returns the corresponding path to the target directory.
	pub fn target_directory(&self, path: &Path) -> PathBuf {
		match self {
			Profile::Release => path.join("target/release"),
			Profile::Debug => path.join("target/debug"),
			Profile::Production => path.join("target/production"),
		}
	}
}

impl fmt::Display for Profile {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Debug => write!(f, "debug"),
			Self::Release => write!(f, "release"),
			Self::Production => write!(f, "production"),
		}
	}
}

