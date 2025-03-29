// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use cliclack::spinner;
use pop_common::{manifest::from_path, Profile};
#[cfg(feature = "parachain")]
use pop_parachains::{
	build_project, get_preset_names, get_runtime_path, runtime_binary_path, GenesisBuilderPolicy,
};
use std::{
	self,
	ffi::OsStr,
	fs,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};

const DEFAULT_RUNTIME_DIR: &str = "./runtime";

/// Runtime features.
pub enum Feature {
	/// `runtime-benchmarks` feature.
	Benchmark,
	/// `try-runtime` feature.
	TryRuntime,
}

impl AsRef<str> for Feature {
	fn as_ref(&self) -> &str {
		match self {
			Feature::Benchmark => "runtime-benchmarks",
			Feature::TryRuntime => "try-runtime",
		}
	}
}

/// Ensures the runtime binary exists. If the binary is not found, it triggers a build process.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `project_path`: The path to the project that contains the runtime.
/// * `mode`: The build profile.
/// * `force`: Whether to force the build process.
#[cfg(feature = "parachain")]
pub fn ensure_runtime_binary_exists(
	cli: &mut impl Cli,
	project_path: &Path,
	mode: &Profile,
	features: Vec<Feature>,
	force: bool,
) -> anyhow::Result<PathBuf> {
	let target_path = mode.target_directory(project_path).join("wbuild");
	let runtime_path = guide_user_to_input_runtime_path(cli, project_path)?;

	// Return if the user has specified a path to the runtime binary.
	if runtime_path.extension() == Some(OsStr::new("wasm")) {
		return Ok(runtime_path);
	}

	// Rebuild the runtime if the binary is not found or the user has forced the build process.
	if force {
		cli.info("Building your runtime...")?;
		return build_runtime(cli, &runtime_path, &target_path, mode, features);
	}

	match runtime_binary_path(&target_path, &runtime_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("📦 Runtime binary was not found. The runtime will be built locally.")?;
			build_runtime(cli, &runtime_path, &target_path, mode, features)
		},
	}
}

#[cfg(feature = "parachain")]
fn build_runtime(
	cli: &mut impl Cli,
	runtime_path: &Path,
	target_path: &Path,
	mode: &Profile,
	features: Vec<Feature>,
) -> anyhow::Result<PathBuf> {
	cli.warning("NOTE: this may take some time...")?;
	let features = features.iter().map(|f| f.as_ref()).collect();
	build_project(runtime_path, None, mode, features, None)?;
	cli.info("\n✅ Runtime built successfully.\n")?;
	runtime_binary_path(target_path, runtime_path).map_err(|e| e.into())
}

/// Guide the user to input a runtime path.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `target_path`: The target path.
#[cfg(feature = "parachain")]
pub fn guide_user_to_input_runtime_path(
	cli: &mut impl Cli,
	target_path: &Path,
) -> anyhow::Result<PathBuf> {
	let mut project_path = match get_runtime_path(target_path) {
		Ok(path) => path,
		Err(_) => {
			cli.warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				target_path.display()
			))?;
			let input: PathBuf = cli
				.input("Please specify the path to the runtime project or the runtime binary.")
				.required(true)
				.default_input(DEFAULT_RUNTIME_DIR)
				.placeholder(DEFAULT_RUNTIME_DIR)
				.interact()?
				.into();
			input.canonicalize()?
		},
	};

	// If a TOML file does not exist, list all directories in the runtime folder and prompt the
	// user to select one.
	if project_path.is_dir() && !project_path.join("Cargo.toml").exists() {
		let runtime = guide_user_to_select_runtime(cli, &project_path)?;
		project_path = project_path.join(runtime);
	}
	Ok(project_path)
}

/// Guide the user to select a runtime project.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `project_path`: Path to the project containing runtimes.
#[cfg(feature = "parachain")]
pub fn guide_user_to_select_runtime(
	cli: &mut impl Cli,
	project_path: &PathBuf,
) -> anyhow::Result<PathBuf> {
	let runtimes = fs::read_dir(project_path)?;
	let mut prompt = cli.select("Select the runtime:");
	for runtime in runtimes {
		let path = runtime?.path();
		if !path.is_dir() {
			continue;
		}
		let manifest = from_path(Some(path.as_path()))?;
		let package = manifest.package();
		let name = package.clone().name;
		let description = package.description().unwrap_or_default().to_string();
		prompt = prompt.item(path, &name, &description);
	}
	Ok(prompt.interact()?)
}

/// Guide the user to select a genesis builder policy.
///
/// # Arguments
/// * `cli`: Command line interface.
#[cfg(feature = "parachain")]
pub fn guide_user_to_select_genesis_policy(
	cli: &mut impl Cli,
	default_value: &Option<GenesisBuilderPolicy>,
) -> anyhow::Result<GenesisBuilderPolicy> {
	let mut prompt = cli
		.select("Select the genesis builder policy:")
		.initial_value(default_value.unwrap_or(GenesisBuilderPolicy::None).to_string());

	let policies: Vec<(String, String)> = GenesisBuilderPolicy::iter()
		.map(|policy| (policy.to_string(), policy.get_documentation().unwrap().to_string()))
		.collect();
	for (policy, description) in policies {
		prompt = prompt.item(policy.clone(), policy.to_string(), description);
	}
	let input = prompt.interact()?;
	GenesisBuilderPolicy::try_from(input).map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Guide the user to select a genesis builder preset.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `runtime_path`: Path to the runtime binary.
/// * `default_value`: Default value of the genesis builder preset.
#[cfg(feature = "parachain")]
pub fn guide_user_to_select_genesis_preset(
	cli: &mut impl Cli,
	runtime_path: &PathBuf,
	default_value: &str,
) -> anyhow::Result<String> {
	let spinner = spinner();
	spinner.start("Loading available genesis builder presets of your runtime...");
	let mut prompt = cli
		.select("Select the genesis builder preset:")
		.initial_value(default_value.to_string());
	let preset_names = get_preset_names(runtime_path)?;
	if preset_names.is_empty() {
		return Err(anyhow::anyhow!("No preset found for the runtime"));
	}
	spinner.stop(format!("Found {} genesis builder presets", preset_names.len()));
	for preset in preset_names {
		prompt = prompt.item(preset.to_string(), preset, "");
	}
	Ok(prompt.interact()?)
}

/// Construct the path to the mock runtime WASM file.
#[cfg(test)]
pub(crate) fn get_mock_runtime(feature: Option<Feature>) -> PathBuf {
	let path = format!(
		"../../tests/runtimes/{}.wasm",
		match feature {
			Some(Feature::Benchmark) => "base_parachain_benchmark",
			Some(Feature::TryRuntime) => "base_parachain_try_runtime",
			_ => "base_parachain",
		}
	);
	std::env::current_dir().unwrap().join(path).canonicalize().unwrap()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use fs::File;
	use strum::VariantArray;
	use tempfile::tempdir;

	#[test]
	fn runtime_feature_ref_works() {
		assert_eq!(Feature::Benchmark.as_ref(), "runtime-benchmarks");
		assert_eq!(Feature::TryRuntime.as_ref(), "try-runtime");
	}

	#[test]
	fn ensure_runtime_binary_exists_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_path = temp_dir.into_path();
		fs::create_dir(&temp_path.join("target"))?;

		for profile in Profile::VARIANTS {
			let target_path = profile.target_directory(temp_path.as_path());
			fs::create_dir(target_path.clone())?;

			// Input path to binary file.
			let binary_path = target_path.join("runtime.wasm");
			let mut cli = expect_input_runtime_path(&temp_path, &binary_path);
			File::create(binary_path.as_path())?;
			assert_eq!(
				ensure_runtime_binary_exists(&mut cli, &temp_path, profile, vec![], true)?,
				binary_path.canonicalize()?
			);
			cli.verify()?;
		}
		Ok(())
	}

	#[test]
	fn guide_user_to_select_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let runtime_path = temp_dir.path().join("runtime");
		let runtime_items = runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec();

		// Found runtimes in the specified runtime path.
		let mut cli = MockCli::new();
		cli = cli.expect_select(
			"Select the runtime:",
			Some(true),
			true,
			Some(runtime_items),
			0,
			None,
		);

		fs::create_dir(&runtime_path)?;
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_select_runtime(&mut cli, &runtime_path)?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_input_runtime_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_path = temp_dir.path().to_path_buf();
		let runtime_path = temp_dir.path().join("runtimes");

		// No runtime path found, ask for manual input from user.
		let runtime_binary_path = temp_path.join("dummy.wasm");
		let mut cli = expect_input_runtime_path(&temp_path, &runtime_binary_path);
		File::create(runtime_binary_path)?;
		guide_user_to_input_runtime_path(&mut cli, &temp_path)?;
		cli.verify()?;

		// Runtime folder found and not a Rust project, select from existing runtimes.
		fs::create_dir(&runtime_path)?;
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let runtime_items = runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec();
		cli = MockCli::new();
		cli = cli.expect_select(
			"Select the runtime:",
			Some(true),
			true,
			Some(runtime_items),
			0,
			None,
		);
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_input_runtime_path(&mut cli, &temp_path)?;

		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_policy_works() -> anyhow::Result<()> {
		// Select genesis builder policy `none`.
		let mut cli = MockCli::new();
		cli = expect_select_genesis_policy(cli, 0);

		guide_user_to_select_genesis_policy(&mut cli, &None)?;
		cli.verify()?;

		// Select genesis builder policy `runtime`.
		let runtime_path = get_mock_runtime(Some(Feature::Benchmark));
		cli = MockCli::new();
		cli = expect_select_genesis_policy(cli, 1);
		cli = expect_select_genesis_preset(cli, &runtime_path, 0);

		guide_user_to_select_genesis_policy(&mut cli, &None)?;
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_preset_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(None);
		let mut cli = MockCli::new();
		cli = expect_select_genesis_preset(cli, &runtime_path, 0);
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	fn expect_input_runtime_path(project_path: &PathBuf, binary_path: &PathBuf) -> MockCli {
		MockCli::new()
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				project_path.display()
			))
			.expect_input(
				"Please specify the path to the runtime project or the runtime binary.",
				binary_path.to_str().unwrap().to_string(),
			)
	}

	fn expect_select_genesis_policy(cli: MockCli, item: usize) -> MockCli {
		let policies: Vec<(String, String)> = GenesisBuilderPolicy::iter()
			.map(|policy| (policy.to_string(), policy.get_documentation().unwrap().to_string()))
			.collect();
		cli.expect_select(
			"Select the genesis builder policy:",
			Some(true),
			true,
			Some(policies),
			item,
			None,
		)
	}

	fn expect_select_genesis_preset(cli: MockCli, runtime_path: &PathBuf, item: usize) -> MockCli {
		let preset_names = get_preset_names(runtime_path)
			.unwrap()
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();
		cli.expect_select(
			"Select the genesis builder preset:",
			Some(true),
			true,
			Some(preset_names),
			item,
			None,
		)
	}
}
