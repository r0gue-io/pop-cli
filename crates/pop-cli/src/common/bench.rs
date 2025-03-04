// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use cliclack::spinner;
use duct::cmd;
use pop_common::{
	get_relative_or_absolute_path, manifest::from_path, set_executable_permission, Profile,
};
use pop_parachains::{
	build_project, get_preset_names, get_runtime_path, omni_bencher_generator, runtime_binary_path,
	GenesisBuilderPolicy,
};
use std::{
	env::current_dir,
	ffi::OsStr,
	fs,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};

const DEFAULT_RUNTIME_DIR: &str = "./runtime";

/// Checks the status of the `frame-omni-bencher` binary, use the local binary if available.
/// Otherwise, sources it if necessary, and prompts the user to update it if the existing binary in
/// cache is not the latest version.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_omni_bencher_and_prompt(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(match cmd("which", &["frame-omni-bencher"]).stdout_capture().run() {
		Ok(output) => {
			let path = String::from_utf8(output.stdout)?;
			PathBuf::from(path.replace("\n", ""))
		},
		Err(_) => source_omni_bencher_binary(cli, cache_path, skip_confirm).await?,
	})
}

/// Prompt to source the `frame-omni-bencher` binary.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn source_omni_bencher_binary(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	let mut binary = omni_bencher_generator(cache_path, None).await?;
	let mut bencher_path = binary.path();
	if !binary.exists() {
		cli.warning("⚠️ The frame-omni-bencher binary is not found.")?;
		let latest = if !skip_confirm {
			cli.confirm("📦 Would you like to source it automatically now?")
				.initial_value(true)
				.interact()?
		} else {
			true
		};
		if latest {
			let spinner = spinner();
			spinner.start("📦 Sourcing frame-omni-bencher...");
			binary.source(false, &(), true).await?;

			spinner.stop(format!(
				"✅ frame-omni-bencher successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			bencher_path = binary.path();
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
			let spinner = spinner();
			spinner.start("📦 Sourcing frame-omni-bencher...");

			binary = omni_bencher_generator(crate::cache()?.as_path(), binary.latest()).await?;
			binary.source(false, &(), true).await?;
			set_executable_permission(binary.path())?;

			spinner.stop(format!(
				"✅ frame-omni-bencher successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			bencher_path = binary.path();
		}
	}
	Ok(bencher_path)
}

/// Ensure that the runtime WASM binary exists and is ready for use. If the binary is not found, it
/// triggers a build process.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `mode`: A reference to the profile mode.
pub fn ensure_runtime_binary_exists(cli: &mut impl Cli, mode: &Profile) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let runtime_path = guide_user_to_select_runtime_path(cli, &cwd)?;

	// Return immediately if the user has specified a path to the runtime binary.
	if runtime_path.extension() == Some(OsStr::new("wasm")) {
		return Ok(runtime_path);
	}

	match runtime_binary_path(&target_path, &runtime_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("📦 Runtime binary was not found. The runtime will be built locally.")?;
			cli.warning("NOTE: this may take some time...")?;
			build_project(&runtime_path, None, mode, vec!["runtime-benchmarks"], None)?;
			runtime_binary_path(&target_path, &runtime_path).map_err(|e| e.into())
		},
	}
}

/// Check the genesis builder policy and prompt the user to select one if there are genesis builder
/// presets available.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `runtime_path`: A reference to the runtime path.
/// * `current_policy`: A mutable reference to the current genesis builder policy.
/// * `current_preset`: A mutable reference to the current genesis builder preset.
pub fn check_genesis_builder_and_prompt(
	cli: &mut impl Cli,
	runtime_path: &PathBuf,
	current_policy: &mut Option<GenesisBuilderPolicy>,
	current_preset: &mut String,
) -> anyhow::Result<()> {
	let preset_names = get_preset_names(runtime_path)?;
	// Determine policy based on preset availability.
	let policy = if preset_names.is_empty() {
		GenesisBuilderPolicy::None
	} else {
		guide_user_to_select_genesis_policy(cli, current_policy)?
	};
	*current_policy = Some(policy);

	// If the policy requires a preset, prompt the user to select one.
	if policy == GenesisBuilderPolicy::Runtime {
		*current_preset = guide_user_to_select_genesis_preset(cli, runtime_path, current_preset)?;
	}
	Ok(())
}

/// Guide the user to select a runtime path.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `target_path`: A reference to the target path.
pub fn guide_user_to_select_runtime_path(
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
				.input("Please provide the path to the runtime or parachain project.")
				.required(true)
				.default_input(DEFAULT_RUNTIME_DIR)
				.placeholder(DEFAULT_RUNTIME_DIR)
				.interact()?
				.into();
			input.canonicalize()?
		},
	};

	// If there is no TOML file exist, list all directories in the "runtime" folder and prompt the
	// user to select a runtime.
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
pub fn guide_user_to_select_runtime(
	cli: &mut impl Cli,
	project_path: &PathBuf,
) -> anyhow::Result<PathBuf> {
	let runtimes = fs::read_dir(project_path).expect("No project found");
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
	Ok(GenesisBuilderPolicy::from(input))
}

/// Guide the user to select a genesis builder preset.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `runtime_path`: Path to the runtime binary.
/// * `default_value`: Default value of the genesis builder preset.
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

/// Get relative path. Returns absolute path if the path is not relative.
pub fn get_relative_path(path: &Path) -> String {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let path = get_relative_or_absolute_path(cwd.as_path(), path);
	path.as_path().to_str().expect("No path provided").to_string()
}

/// Construct the path to the mock runtime WASM file.
#[cfg(test)]
pub(crate) fn get_mock_runtime(with_benchmark_features: bool) -> PathBuf {
	let path = format!(
		"../../tests/runtimes/{}.wasm",
		if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
	);
	current_dir().unwrap().join(path).canonicalize().unwrap()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use tempfile::tempdir;

	#[tokio::test]
	async fn source_omni_bencher_binary_works() -> anyhow::Result<()> {
		let cache_path = tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning("⚠️ The frame-omni-bencher binary is not found.")
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning("⚠️ The frame-omni-bencher binary is not found.");

		let path = source_omni_bencher_binary(&mut cli, cache_path.path(), false).await?;
		// Binary path is at least equal to the cache path + "frame-omni-bencher".
		assert!(path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("frame-omni-bencher").to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn source_omni_bencher_binary_handles_skip_confirm() -> anyhow::Result<()> {
		let cache_path = tempdir().expect("Could create temp dir");
		let mut cli =
			MockCli::new().expect_warning("⚠️ The frame-omni-bencher binary is not found.");

		let path = source_omni_bencher_binary(&mut cli, cache_path.path(), true).await?;
		// Binary path is at least equal to the cache path + "frame-omni-bencher".
		assert!(path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("frame-omni-bencher").to_str().unwrap()));
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let runtime_path = temp_dir.path().join("runtime");
		let runtime_items = runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec();

		// Found runtimes in the specified runtime path.
		let mut cli = MockCli::new();
		cli = cli.expect_select("Select the runtime:", Some(true), true, Some(runtime_items), 0);

		fs::create_dir(&runtime_path)?;
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_select_runtime(&mut cli, &runtime_path)?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_runtime_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_path = temp_dir.path().to_path_buf();
		let runtime_path = temp_dir.path().join("runtimes");

		// No runtime path found, ask for manual input from user.
		let mut cli = MockCli::new();
		let runtime_binary_path = temp_path.join("dummy.wasm");
		cli = cli.expect_warning(format!(
			"No runtime folder found at {}. Please input the runtime path manually.",
			temp_path.display()
		));
		cli = cli.expect_input(
			"Please provide the path to the runtime or parachain project.",
			runtime_binary_path.to_str().unwrap().to_string(),
		);
		fs::File::create(runtime_binary_path)?;
		guide_user_to_select_runtime_path(&mut cli, &temp_path)?;
		cli.verify()?;

		// Runtime folder found and not a Rust project, select from existing runtimes.
		fs::create_dir(&runtime_path)?;
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let runtime_items = runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec();
		cli = MockCli::new();
		cli = cli.expect_select("Select the runtime:", Some(true), true, Some(runtime_items), 0);
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_select_runtime_path(&mut cli, &temp_path)?;

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
		let runtime_path = get_mock_runtime(true);
		cli = MockCli::new();
		cli = expect_select_genesis_policy(cli, 1);
		cli = expect_select_genesis_preset(cli, &runtime_path, 0);

		guide_user_to_select_genesis_policy(&mut cli, &None)?;
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_preset_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(false);
		let mut cli = MockCli::new();
		cli = expect_select_genesis_preset(cli, &runtime_path, 0);
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
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
		)
	}
}
