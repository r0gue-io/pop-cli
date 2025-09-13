// SPDX-License-Identifier: GPL-3.0

use duct::cmd;
#[cfg(any(feature = "chain", test))]
use std::cmp::Ordering;
#[cfg(any(
	feature = "polkavm-contracts",
	feature = "wasm-contracts",
	feature = "chain",
	test
))]
use std::path::PathBuf;
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "chain"))]
use {
	crate::cli::traits::*,
	cliclack::spinner,
	pop_common::sourcing::{set_executable_permission, Binary},
	std::path::Path,
};

/// A trait for binary generator.
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "chain"))]
pub(crate) trait BinaryGenerator {
	/// Generates a binary.
	///
	/// # Arguments
	/// * `cache_path` - The cache directory path where the binary is stored.
	/// * `version` - The specific version used for the binary (`None` to fetch the latest version).
	async fn generate(
		cache_path: PathBuf,
		version: Option<&str>,
	) -> Result<Binary, pop_common::Error>;
}

/// Fetches the binary in a temporary directory to avoid conflicts with the cache directory.
///
/// Especially useful for tests that concurrently fetch the same binary.
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "chain"))]
async fn safe_fetch_binary<Generator: BinaryGenerator>(path: &Path) -> anyhow::Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let temp_binary = Generator::generate(temp_dir.path().to_path_buf(), None).await?;
	temp_binary.source(false, &(), true).await?;
	set_executable_permission(temp_binary.path())?;
	if std::fs::rename(temp_binary.path(), path).is_err() {
		let _ = std::fs::copy(temp_binary.path(), path);
	}
	Ok(())
}

/// Checks the status of the provided binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `cli` - Command-line interface for user interaction.
/// * `binary_name` - The name of the binary to check.
/// * `cache_path` - The cache directory path where the binary is stored.
/// * `skip_confirm` - If `true`, skips confirmation prompts and automatically sources the binary if
///   needed.
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "chain"))]
pub async fn check_and_prompt<Generator: BinaryGenerator>(
	cli: &mut impl Cli,
	binary_name: &'static str,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	let mut binary = Generator::generate(PathBuf::from(cache_path), None).await?;
	let mut binary_path = binary.path();
	if !binary.exists() {
		cli.warning(format!("⚠️ The {binary_name} binary is not found."))?;
		let latest = if !skip_confirm {
			cli.confirm("📦 Would you like to source it automatically now?")
				.initial_value(true)
				.interact()?
		} else {
			true
		};
		if latest {
			let spinner = spinner();
			spinner.start(format!("📦 Sourcing {binary_name}..."));
			safe_fetch_binary::<Generator>(&binary.path()).await?;
			spinner.stop(format!(
				"✅ {binary_name} successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			binary_path = binary.path();
		}
	}
	if binary.stale() {
		cli.warning(format!(
			"ℹ️ There is a newer version of {} available:\n {} -> {}",
			binary.name(),
			binary.version().unwrap_or("None"),
			binary.latest().unwrap_or("None")
		))?;
		let latest = if !skip_confirm {
			cli.confirm(
				"📦 Would you like to source it automatically now? It may take some time..."
					.to_string(),
			)
			.initial_value(true)
			.interact()?
		} else {
			true
		};
		if latest {
			binary = Generator::generate(crate::cache()?, binary.latest()).await?;
			let spinner = spinner();
			spinner.start(format!("📦 Sourcing {binary_name}..."));
			safe_fetch_binary::<Generator>(&binary.path()).await?;
			spinner.stop(format!(
				"✅ {binary_name} successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			binary_path = binary.path();
		}
	}

	Ok(binary_path)
}

/// A macro to implement a binary generator.
#[macro_export]
macro_rules! impl_binary_generator {
	($generator_name:ident, $generate_fn:ident) => {
		pub(crate) struct $generator_name;

		impl BinaryGenerator for $generator_name {
			async fn generate(
				cache_path: PathBuf,
				version: Option<&str>,
			) -> Result<Binary, pop_common::Error> {
				$generate_fn(cache_path, version).await
			}
		}
	};
}

/// Represents a semantic version (major.minor.patch).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SemanticVersion(pub u8, pub u8, pub u8);

impl TryFrom<String> for SemanticVersion {
	type Error = anyhow::Error;
	fn try_from(binary: String) -> Result<Self, Self::Error> {
		match cmd(binary, vec!["--version"])
			.pipe(cmd("grep", vec!["-oE", r"[0-9]+\.[0-9]+\.[0-9]+"]))
			.read()
		{
			Ok(version) => {
				let version = version.trim();
				let parts: Vec<&str> = version.split('.').collect();
				if parts.len() == 3 {
					let major = parts[0].parse::<u8>()?;
					let minor = parts[1].parse::<u8>()?;
					let patch = parts[2].parse::<u8>()?;
					Ok(SemanticVersion(major, minor, patch))
				} else {
					Err(anyhow::anyhow!("Invalid version format"))
				}
			},
			Err(e) => Err(anyhow::anyhow!("Failed to get version: {}", e)),
		}
	}
}

/// Finds the path to a binary matches a specific version.
///
/// # Arguments
///
/// * `binary` - The name of the binary to find.
/// * `target_version` - The version to match.
/// * `order` - The ordering to use when matching versions.
#[cfg(any(feature = "chain", test))]
pub(crate) fn which_version(
	binary: &str,
	target_version: &SemanticVersion,
	order: &Ordering,
) -> anyhow::Result<PathBuf> {
	match cmd("which", &[binary]).read() {
		Ok(path) => {
			let path = path.trim();
			let version = SemanticVersion::try_from(path.to_string().clone())?;
			if version.cmp(target_version) == *order {
				Ok(PathBuf::from(path))
			} else {
				Err(anyhow::anyhow!("Binary version does not match target version"))
			}
		},
		Err(_) => Err(anyhow::anyhow!("Failed to find binary")),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cmp::Ordering;

	#[test]
	fn semantic_version_works() {
		assert!(SemanticVersion::try_from("bash".to_string()).is_ok());
	}

	#[test]
	fn semantic_version_invalid_binary() {
		assert_eq!(
			SemanticVersion::try_from("./dummy-binary".to_string()).unwrap_err().to_string(),
			"Failed to get version: No such file or directory (os error 2)".to_string()
		);
	}

	#[test]
	fn which_version_works() {
		assert_eq!(
			which_version(
				"bash",
				&SemanticVersion::try_from("bash".to_string()).unwrap(),
				&Ordering::Equal,
			)
			.unwrap()
			.to_str()
			.unwrap()
			.to_string(),
			cmd("which", &["bash"]).read().unwrap(),
		);
		assert_eq!(
			which_version("bash", &SemanticVersion(0, 0, 0), &Ordering::Greater)
				.unwrap()
				.to_str()
				.unwrap()
				.to_string(),
			cmd("which", &["bash"]).read().unwrap(),
		);
		assert_eq!(
			which_version("bash", &SemanticVersion(0, 0, 0), &Ordering::Less)
				.unwrap_err()
				.to_string(),
			"Binary version does not match target version".to_string()
		);
		assert_eq!(
			which_version("no-binary-found", &SemanticVersion(0, 0, 0), &Ordering::Less)
				.unwrap_err()
				.to_string(),
			"Failed to find binary".to_string()
		);
	}
}
