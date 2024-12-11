// SPDX-License-Identifier: GPL-3.0

use std::{
	fmt,
	path::{Path, PathBuf},
};
use strum_macros::{AsRefStr, EnumMessage, EnumString, VariantArray};

/// Enum representing a build profile.
#[derive(AsRefStr, Clone, Default, Debug, EnumString, EnumMessage, VariantArray, Eq, PartialEq)]
pub enum Profile {
	/// Debug profile, optimized for debugging.
	#[strum(serialize = "debug", message = "Debug", detailed_message = "Optimized for debugging.")]
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

impl From<Profile> for bool {
	fn from(value: Profile) -> Self {
		value != Profile::Debug
	}
}

impl From<bool> for Profile {
	fn from(value: bool) -> Self {
		if value {
			Profile::Release
		} else {
			Profile::Debug
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

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::Path;
	use strum::EnumMessage;

	#[test]
	fn profile_from_string() {
		assert_eq!("debug".parse::<Profile>().unwrap(), Profile::Debug);
		assert_eq!("release".parse::<Profile>().unwrap(), Profile::Release);
		assert_eq!("production".parse::<Profile>().unwrap(), Profile::Production);
	}

	#[test]
	fn profile_detailed_message() {
		assert_eq!(Profile::Debug.get_detailed_message(), Some("Optimized for debugging."));
		assert_eq!(
			Profile::Release.get_detailed_message(),
			Some("Optimized without any debugging functionality.")
		);
		assert_eq!(
			Profile::Production.get_detailed_message(),
			Some("Optimized for ultimate performance.")
		);
	}

	#[test]
	fn profile_target_directory() {
		let base_path = Path::new("/example/path");

		assert_eq!(
			Profile::Debug.target_directory(base_path),
			Path::new("/example/path/target/debug")
		);
		assert_eq!(
			Profile::Release.target_directory(base_path),
			Path::new("/example/path/target/release")
		);
		assert_eq!(
			Profile::Production.target_directory(base_path),
			Path::new("/example/path/target/production")
		);
	}

	#[test]
	fn profile_default() {
		let default_profile = Profile::default();
		assert_eq!(default_profile, Profile::Release);
	}

	#[test]
	fn profile_from_bool() {
		assert_eq!(Profile::from(true), Profile::Release);
		assert_eq!(Profile::from(false), Profile::Debug);
	}

	#[test]
	fn profile_into_bool() {
		assert_eq!(bool::from(Profile::Debug), false);
		assert_eq!(bool::from(Profile::Release), true);
		assert_eq!(bool::from(Profile::Production), true);
	}
}
