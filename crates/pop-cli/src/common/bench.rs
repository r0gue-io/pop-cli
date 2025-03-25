// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	common::binary::{check_and_prompt, BinaryGenerator},
	impl_binary_generator,
};
use cliclack::spinner;
use duct::cmd;
use pop_common::{manifest::from_path, sourcing::Binary, Profile};
use pop_parachains::{
	build_project, get_preset_names, get_runtime_path, omni_bencher_generator, runtime_binary_path,
	GenesisBuilderPolicy,
};
use std::{
	self,
	ffi::OsStr,
	fs,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};

const DEFAULT_RUNTIME_DIR: &str = "./runtime";
pub(crate) const EXECUTED_COMMAND_COMMENT: &str = "// Executed Command:";

impl_binary_generator!(OmniBencherGenerator, omni_bencher_generator);

/// Checks the status of the `frame-omni-bencher` binary, using the local version if available.
/// If the binary is missing, it is sourced as needed, and if an outdated version exists in cache,
/// the user is prompted to update to the latest release.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_omni_bencher_and_prompt(
	cli: &mut impl Cli,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(match cmd("which", &["frame-omni-bencher"]).stdout_capture().run() {
		Ok(output) => {
			let path = String::from_utf8(output.stdout)?;
			PathBuf::from(path.trim())
		},
		Err(_) => source_omni_bencher_binary(cli, &crate::cache()?, skip_confirm).await?,
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
	Ok(check_and_prompt::<OmniBencherGenerator>(
		cli,
		"frame-omni-bencher",
		cache_path,
		skip_confirm,
	)
	.await?)
}

/// Ensure the runtime binary exists. If the binary is not found, it triggers a build process.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `project_path`: The path to the project that contains the runtime.
/// * `mode`: The build profile.
/// * `force`: Whether to force the build process.
pub fn ensure_runtime_binary_exists(
	cli: &mut impl Cli,
	project_path: &Path,
	mode: &Profile,
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
		return build_runtime_benchmark(cli, &runtime_path, &target_path, mode);
	}

	match runtime_binary_path(&target_path, &runtime_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("📦 Runtime binary was not found. The runtime will be built locally.")?;
			build_runtime_benchmark(cli, &runtime_path, &target_path, mode)
		},
	}
}

fn build_runtime_benchmark(
	cli: &mut impl Cli,
	runtime_path: &Path,
	target_path: &Path,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	cli.warning("NOTE: this may take some time...")?;
	build_project(runtime_path, None, mode, vec!["runtime-benchmarks"], None)?;
	runtime_binary_path(target_path, runtime_path).map_err(|e| e.into())
}

/// Guide the user to input a runtime path.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `target_path`: The target path.
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

	// If a TOML file does not exist, list all directories in the "runtime" folder and prompt the
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
pub(crate) fn get_mock_runtime(with_benchmark_features: bool) -> PathBuf {
	let path = format!(
		"../../tests/runtimes/{}.wasm",
		if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
	);
	std::env::current_dir().unwrap().join(path).canonicalize().unwrap()
}

/// Overwrite the generated weight files' executed command in the destination directory.
///
/// # Arguments
/// * `temp_path`: The path to the temporary directory.
/// * `dest_path`: The path to the destination directory.
/// * `arguments`: The arguments to overwrite the weight directory with.
pub(crate) fn overwrite_weight_dir_command(
	temp_path: &Path,
	dest_path: &Path,
	arguments: &[String],
) -> anyhow::Result<()> {
	// Create the destination directory if it doesn't exist.
	if !dest_path.is_dir() {
		fs::create_dir(dest_path)?;
	}

	// Read and print contents of all files in the temporary directory.
	for entry in temp_path.read_dir()? {
		let path = entry?.path();
		if !path.is_file() {
			continue;
		}

		let destination = dest_path.join(path.file_name().unwrap());
		overwrite_weight_file_command(&path, destination.as_path(), arguments)?;
	}
	Ok(())
}

/// Overwrites the weight file's executed command with the given arguments.
///
/// # Arguments
/// * `temp_file` - The path to the temporary file.
/// * `dest_file` - The path to the destination file.
/// * `arguments` - The arguments to write to the file.
pub(crate) fn overwrite_weight_file_command(
	temp_file: &Path,
	dest_file: &Path,
	arguments: &[String],
) -> anyhow::Result<()> {
	let contents = fs::read_to_string(temp_file)?;
	let lines: Vec<&str> = contents.split("\n").collect();
	let mut iter = lines.iter();
	let mut new_lines: Vec<String> = vec![];

	let mut inside_command_block = false;
	for line in iter.by_ref() {
		if line.starts_with(EXECUTED_COMMAND_COMMENT) {
			inside_command_block = true;
			continue;
		} else if inside_command_block {
			if line.starts_with("//") {
				continue;
			} else if line.trim().is_empty() {
				// Write new command block to the generated weight file.
				new_lines.push(EXECUTED_COMMAND_COMMENT.to_string());
				for argument in arguments {
					new_lines.push(format!("//  {}", argument));
				}
				new_lines.push(String::new());
				break;
			}
		}
		new_lines.push(line.to_string());
	}

	// Write the rest of the file to the destination file.
	for line in iter {
		new_lines.push(line.to_string());
	}

	fs::write(dest_file, new_lines.join("\n"))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use fs::File;
	use strum::VariantArray;
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
				ensure_runtime_binary_exists(&mut cli, &temp_path, profile, true)?,
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

	#[test]
	fn overwrite_weight_dir_command_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let dest_dir = tempdir()?;
		let files = ["weights-1.rs", "weights-2.rs", "weights-3.rs"];

		for file in files {
			let temp_file = temp_dir.path().join(file);
			fs::write(temp_file.clone(), "// Executed Command:\n// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.")?;
		}

		overwrite_weight_dir_command(
			temp_dir.path(),
			dest_dir.path(),
			&vec!["new".to_string(), "command".to_string(), "replaced".to_string()],
		)?;

		for file in files {
			let dest_file = dest_dir.path().join(file);
			assert_eq!(fs::read_to_string(dest_file)?, "// Executed Command:\n//  new\n//  command\n//  replaced\n\nThis line should not be replaced.");
		}

		Ok(())
	}

	#[test]
	fn overwrite_weight_file_command_works() -> anyhow::Result<()> {
		for (original, expected) in [
			(
				"// Executed Command:\n// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
				"// Executed Command:\n//  new\n//  command\n//  replaced\n\nThis line should not be replaced."
			),
			// Not replace because not "Executed Commnad" comment block found.
			(
				"// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
				"// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
			),
			// Not replacing contents before the "Executed Command" comment block.
			(
    			"Before line should not be replaced\n\n// Executed Command:\n// command\n// should\n// be\n// replaced\n\nAfter line should not be replaced.",
    			"Before line should not be replaced\n\n// Executed Command:\n//  new\n//  command\n//  replaced\n\nAfter line should not be replaced.",
			),
		] {
			let temp_dir = tempdir()?;
			let dest_dir = tempdir()?;
			let temp_file = temp_dir.path().join("weights.rs");
			fs::write(
    			temp_file.clone(),
    			original
    		)?;
			let dest_file = dest_dir.path().join("dest_weights.rs");
			File::create(dest_file.clone())?;

			overwrite_weight_file_command(
				&temp_file,
				dest_file.as_path(),
				&vec!["new".to_string(), "command".to_string(), "replaced".to_string()],
			)?;

			let content = fs::read_to_string(dest_file)?;
			assert_eq!(
    			content,
    			expected
    		);
		}
		Ok(())
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
