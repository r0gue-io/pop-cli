use std::{
	fmt,
	path::{Path, PathBuf},
};

/// Enum representing a build profile.
#[derive(Debug, PartialEq)]
pub enum Profile {
	/// Debug profile, optimized for debugging.
	Debug,
	/// Release profile, optimized without any debugging functionality.
	Release,
}

impl Profile {
	/// Returns the corresponding path to the target folder.
	pub fn target_folder(&self, path: &Path) -> PathBuf {
		match self {
			Profile::Release => path.join("target/release"),
			Profile::Debug => path.join("target/debug"),
		}
	}
}

impl From<bool> for Profile {
	fn from(release: bool) -> Self {
		if release {
			Profile::Release
		} else {
			Profile::Debug
		}
	}
}

impl fmt::Display for Profile {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Debug => write!(f, "Debug"),
			Self::Release => write!(f, "Release"),
		}
	}
}
