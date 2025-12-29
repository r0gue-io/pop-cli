// SPDX-License-Identifier: GPL-3.0

use crate::{cli::traits::*, common::builds::find_runtime_dir};
use cliclack::spinner;
use console::style;
use pop_chains::utils::helpers::get_preset_names;
#[cfg(feature = "chain")]
use pop_chains::{DeterministicBuilder, GenesisBuilderPolicy, build_project, runtime_binary_path};
use pop_common::{Profile, manifest::from_path};
use std::{
	self,
	cmp::Ordering,
	ffi::OsStr,
	fs,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};

/// Runtime features.
#[derive(PartialEq, Eq, Clone, Hash)]
pub enum Feature {
	/// `runtime-benchmarks` feature.
	Benchmark,
	/// `try-runtime` feature.
	TryRuntime,
	/// Other feature.
	Other(String),
}

impl PartialOrd for Feature {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for Feature {
	fn cmp(&self, other: &Self) -> Ordering {
		self.as_ref().cmp(other.as_ref())
	}
}

impl AsRef<str> for Feature {
	fn as_ref(&self) -> &str {
		match self {
			Self::Benchmark => "runtime-benchmarks",
			Self::TryRuntime => "try-runtime",
			Self::Other(value) => value.as_str(),
		}
	}
}

impl From<&str> for Feature {
	fn from(value: &str) -> Self {
		match value {
			"runtime-benchmarks" => Self::Benchmark,
			"try-runtime" => Self::TryRuntime,
			_ => Self::Other(value.to_string()),
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
#[cfg(feature = "chain")]
pub async fn ensure_runtime_binary_exists(
	cli: &mut impl Cli,
	project_path: &Path,
	mode: &Profile,
	features: &[Feature],
	force: bool,
	deterministic: bool,
	default_runtime_path: &Option<PathBuf>,
	tag: Option<String>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
	let target_path = mode.target_directory(project_path).join("wbuild");
	let runtime_path = match default_runtime_path {
		Some(path) => path.clone(),
		None => match find_runtime_dir(project_path, cli) {
			Ok(path) => path,
			Err(_) => {
				cli.warning(format!(
					"No runtime folder found at {}. Please input the runtime path manually.",
					project_path.display()
				))?;
				let input: PathBuf = cli
					.input("Please, specify the path to the runtime project or the runtime binary.")
					.required(true)
					.interact()?
					.into();
				input.canonicalize()?
			},
		},
	};
	cli.info(format!("Using runtime at {}", runtime_path.display()))?;

	// Return if the user has specified a path to the runtime binary.
	if runtime_path.extension() == Some(OsStr::new("wasm")) {
		return Ok((runtime_path.clone(), runtime_path));
	}
	// Rebuild the runtime if the binary is not found or the user has forced the build process.
	if force {
		return build_runtime(cli, &runtime_path, &target_path, mode, features, deterministic, tag)
			.await;
	}
	match runtime_binary_path(&target_path, &runtime_path) {
		Ok(binary_path) => Ok((binary_path, runtime_path)),
		_ => {
			cli.info("📦 Runtime binary was not found. The runtime will be built locally.")?;
			build_runtime(cli, &runtime_path, &target_path, mode, features, deterministic, tag)
				.await
		},
	}
}

/// Build a runtime. Returns the path to the runtime binary and the path to the runtime source.
#[cfg(feature = "chain")]
pub(crate) async fn build_runtime(
	cli: &mut impl Cli,
	runtime_path: &Path,
	target_path: &Path,
	mode: &Profile,
	features: &[Feature],
	deterministic: bool,
	tag: Option<String>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
	cli.warning("NOTE: this may take some time...")?;
	let binary_path = if deterministic {
		let spinner = spinner();
		let manifest = from_path(runtime_path)?;
		let package = manifest.package.as_ref().ok_or(anyhow::anyhow!(format!(
			"Couldn't find package declaration at {:?}",
			runtime_path
		)))?;
		let name = package.clone().name;
		spinner.start("Building deterministic runtime...");
		build_deterministic_runtime(&name, *mode, runtime_path.to_path_buf(), tag)
			.await?
			.0
	} else {
		cli.info(format!("Building your runtime in {mode} mode..."))?;
		let features: Vec<String> = features.iter().map(|f| f.as_ref().to_string()).collect();
		build_project(runtime_path, None, mode, &features, None)?;
		runtime_binary_path(target_path, runtime_path)?
	};
	cli.info(format!("The runtime was built in {mode} mode."))?;
	cli.success("\n✅ Runtime built successfully.\n")?;
	print_build_output(cli, &binary_path)?;
	Ok((binary_path, runtime_path.into()))
}

#[cfg(feature = "chain")]
fn print_build_output(cli: &mut impl Cli, binary_path: &Path) -> anyhow::Result<()> {
	let generated_files = [format!("Binary generated at: {}", binary_path.display())];
	let generated_files: Vec<_> = generated_files
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
	cli.success(format!("Generated files:\n{}", generated_files.join("\n")))?;
	cli.outro(format!(
		"Need help? Learn more at {}\n",
		style("https://learn.onpop.io").magenta().underlined()
	))?;
	Ok(())
}

/// Build a deterministic runtime.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `spinner`: The progress bar.
/// * `package`: The package name.
/// * `profile`: The build profile.
/// * `runtime_dir`: The runtime directory.
#[cfg(feature = "chain")]
pub(crate) async fn build_deterministic_runtime(
	package: &str,
	profile: Profile,
	runtime_dir: PathBuf,
	tag: Option<String>,
) -> anyhow::Result<(PathBuf, Vec<u8>)> {
	let runtime_path = {
		let builder = DeterministicBuilder::new(None, package, profile, runtime_dir, tag).await?;
		let wasm_path = builder.build()?;
		if !wasm_path.exists() {
			return Err(anyhow::anyhow!("Can't find the generated runtime at {:?}", wasm_path));
		};
		Ok(wasm_path)
	}
	.map_err(|e: anyhow::Error| {
		anyhow::anyhow!("Failed to build the deterministic runtime: {e}")
	})?;
	let code = fs::read(&runtime_path).map_err(anyhow::Error::from)?;
	Ok((runtime_path, code))
}

/// Guide the user to select a genesis builder policy.
///
/// # Arguments
/// * `cli`: Command line interface.
#[cfg(feature = "chain")]
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
#[cfg(feature = "chain")]
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
	use pop_common::manifest::{add_feature, add_production_profile};
	use strum::VariantArray;
	use tempfile::tempdir;

	#[test]
	fn runtime_feature_ref_works() {
		assert_eq!(Feature::Benchmark.as_ref(), "runtime-benchmarks");
		assert_eq!(Feature::TryRuntime.as_ref(), "try-runtime");
	}

	#[tokio::test]
	async fn ensure_runtime_binary_exists_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_path = temp_dir.path().to_path_buf();
		fs::create_dir(temp_path.join("target"))?;

		for profile in Profile::VARIANTS {
			let target_path = profile.target_directory(temp_path.as_path());
			fs::create_dir(target_path.clone())?;

			// Input path to binary file.
			let binary_path = target_path.join("runtime.wasm");
			File::create(binary_path.as_path())?;
			let mut cli = expect_input_runtime_path(&temp_path, &binary_path);
			assert_eq!(
				ensure_runtime_binary_exists(
					&mut cli,
					&temp_path,
					profile,
					&[],
					true,
					false,
					&None,
					None
				)
				.await?,
				(binary_path.canonicalize()?, binary_path.canonicalize()?)
			);
			cli.verify()?;

			// Provide a path to a runtime binary.
			assert_eq!(
				ensure_runtime_binary_exists(
					&mut MockCli::new(),
					&temp_path,
					profile,
					&[],
					true,
					false,
					&Some(binary_path.canonicalize()?),
					None
				)
				.await?,
				(binary_path.canonicalize()?, binary_path.canonicalize()?)
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn build_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let path = temp_dir.path();
		let runtime_name = "mock_runtime";
		cmd("cargo", ["new", "--lib", runtime_name]).dir(path).run()?;

		// Create a runtime directory
		let target_dir = path.join(runtime_name);
		add_feature(target_dir.as_path(), ("try-runtime".to_string(), vec![]))?;
		add_feature(target_dir.as_path(), ("runtime-benchmarks".to_string(), vec![]))?;

		let project_path = path.join(runtime_name);
		let features = vec![Feature::Benchmark, Feature::TryRuntime];
		add_production_profile(&project_path)?;
		for feature in features.iter() {
			add_feature(&project_path, (feature.as_ref().to_string(), vec![]))?;
		}

		for profile in Profile::VARIANTS {
			for features in [vec![], vec![Feature::Benchmark], features.clone()] {
				let target_path = profile.target_directory(&target_dir).join("wbuild");
				let binary_path =
					target_path.join(format!("{}/{}.wasm", runtime_name, runtime_name));
				fs::create_dir_all(&binary_path)?;

				let generated_files = [format!("Binary generated at: {}", binary_path.display())];
				let generated_files: Vec<_> = generated_files
					.iter()
					.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
					.collect();
				let mut cli = MockCli::new()
					.expect_warning("NOTE: this may take some time...")
					.expect_info(format!("Building your runtime in {profile} mode..."))
					.expect_info(format!("The runtime was built in {profile} mode."))
					.expect_success("\n✅ Runtime built successfully.\n")
					.expect_success(format!("Generated files:\n{}", generated_files.join("\n")))
					.expect_outro(format!(
						"Need help? Learn more at {}\n",
						style("https://learn.onpop.io").magenta().underlined()
					));
				build_runtime(
					&mut cli,
					&project_path,
					&target_path,
					profile,
					&features,
					false,
					None,
				)
				.await?;
				cli.verify()?;
			}
		}
		Ok(())
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
	fn print_build_output_works() -> anyhow::Result<()> {
		let binary_path = PathBuf::from("./dummy-runtime.wasm");
		let generated_files = [format!("Binary generated at: {}", binary_path.display())];
		let generated_files: Vec<_> = generated_files
			.iter()
			.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
			.collect();
		let mut cli = MockCli::new()
			.expect_success(format!("Generated files:\n{}", generated_files.join("\n")))
			.expect_outro(format!(
				"Need help? Learn more at {}\n",
				style("https://learn.onpop.io").magenta().underlined()
			));
		print_build_output(&mut cli, &binary_path)?;
		cli.verify()?;
		Ok(())
	}

	fn expect_input_runtime_path(project_path: &Path, binary_path: &Path) -> MockCli {
		let canonical_path = binary_path.canonicalize().unwrap();
		MockCli::new()
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				project_path.display()
			))
			.expect_input(
				"Please, specify the path to the runtime project or the runtime binary.",
				binary_path.to_str().unwrap().to_string(),
			)
			.expect_info(format!("Using runtime at {}", canonical_path.display()))
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
