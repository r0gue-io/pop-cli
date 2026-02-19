// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	cli::traits::Cli as _,
	output::{BuildCommandError, CliResponse, OutputMode, PromptRequiredError},
};
use clap::{Args, Subcommand};
use duct::cmd;
#[cfg(any(feature = "chain", feature = "contract"))]
use pop_common::Docker;
use pop_common::Profile;
use serde::Serialize;
use std::{
	path::{Path, PathBuf},
	process::Output as ProcessOutput,
};
#[cfg(feature = "chain")]
use {
	chain::BuildChain,
	runtime::{BuildRuntime, Feature::*},
	spec::BuildSpecCommand,
	std::fmt::{Display, Formatter, Result},
};
#[cfg(feature = "contract")]
use {
	contract::BuildContract,
	pop_contracts::{BuildMode, MetadataSpec},
};

#[cfg(feature = "chain")]
pub(crate) mod chain;
#[cfg(feature = "contract")]
pub(crate) mod contract;
#[cfg(feature = "chain")]
pub(crate) mod runtime;
#[cfg(feature = "chain")]
pub(crate) mod spec;

#[cfg(feature = "chain")]
const CHAIN_HELP_HEADER: &str = "Chain options";
#[cfg(feature = "chain")]
const RUNTIME_HELP_HEADER: &str = "Runtime options";
#[cfg(feature = "contract")]
const CONTRACT_HELP_HEADER: &str = "Contract options";
const PACKAGE: &str = "package";
#[cfg(feature = "chain")]
const PARACHAIN: &str = "parachain";
const PROJECT: &str = "project";
const JSON_PROMPT_ERR: &str = "interactive prompt required but --json mode is active";

/// Structured output for `build --json`.
#[derive(Debug, Serialize)]
pub(crate) struct BuildOutput {
	artifact_path: String,
	profile: String,
	features: Vec<String>,
}

/// Entry point called from the command dispatcher.
pub(crate) async fn execute(args: &BuildArgs, output_mode: OutputMode) -> anyhow::Result<()> {
	#[cfg(feature = "chain")]
	if let Some(command) = &args.command {
		return match output_mode {
			OutputMode::Human => match command {
				Command::Spec(cmd) => cmd.execute(OutputMode::Human).await,
			},
			OutputMode::Json => match command {
				Command::Spec(cmd) => cmd.execute(OutputMode::Json).await,
			},
		};
	}

	match output_mode {
		OutputMode::Human => Command::execute(args).await,
		OutputMode::Json => Command::execute_json(args).await,
	}
}

/// Arguments for building a project.
#[derive(Args, Serialize)]
#[cfg_attr(test, derive(Default))]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
	#[command(subcommand)]
	#[cfg(feature = "chain")]
	pub command: Option<Command>,
	/// Directory path with flag for your project [default: current directory]
	#[serde(skip_serializing)]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[serde(skip_serializing)]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The package to be built.
	#[arg(short = 'p', long)]
	pub(crate) package: Option<String>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short, long, conflicts_with = "profile")]
	pub(crate) release: bool,
	/// Build profile [default: debug].
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// List of features that project is built with, separated by commas.
	#[clap(short, long)]
	pub(crate) features: Option<String>,
	/// For benchmarking, always build with `runtime-benchmarks` feature.
	#[clap(short, long, help_heading = CHAIN_HELP_HEADER)]
	#[cfg(feature = "chain")]
	pub(crate) benchmark: bool,
	/// For testing with `try-runtime`, always build with `try-runtime` feature.
	#[clap(short, long, help_heading = CHAIN_HELP_HEADER)]
	#[cfg(feature = "chain")]
	pub(crate) try_runtime: bool,
	/// Whether to build a runtime deterministically. Implies --only-runtime.
	#[clap(short, long, help_heading = RUNTIME_HELP_HEADER)]
	#[cfg(feature = "chain")]
	pub(crate) deterministic: bool,
	/// Whether to use a specific image tag for a deterministic build (srtool image)
	#[clap(long, requires = "deterministic", help_heading = RUNTIME_HELP_HEADER)]
	#[cfg(feature = "chain")]
	pub(crate) tag: Option<String>,
	/// Whether to build only the runtime.
	#[clap(long, help_heading = RUNTIME_HELP_HEADER)]
	#[cfg(feature = "chain")]
	pub(crate) only_runtime: bool,
	/// Which specification to use for contract metadata.
	#[clap(long, help_heading = CONTRACT_HELP_HEADER)]
	#[cfg(feature = "contract")]
	pub(crate) metadata: Option<MetadataSpec>,
	/// Whether to build in a way that the contract is verifiable.
	#[clap(long, conflicts_with_all = ["release", "profile"], help_heading = CONTRACT_HELP_HEADER)]
	#[cfg(feature = "contract")]
	pub(crate) verifiable: bool,
	/// Custom image for verifiable builds.
	#[clap(long, requires = "verifiable", help_heading = CONTRACT_HELP_HEADER)]
	#[cfg(feature = "contract")]
	pub(crate) image: Option<String>,
}

impl BuildArgs {
	fn display(&self) -> String {
		let mut full_message = "pop build".to_string();
		if let Some(path) = &self.path {
			full_message.push_str(&format!(" --path {}", path.display()));
		}
		if let Some(path_pos) = &self.path_pos {
			full_message.push_str(&format!(" {}", path_pos.display()));
		}
		if let Some(package) = &self.package {
			full_message.push_str(&format!(" --package {}", package));
		}
		if self.release {
			full_message.push_str(" --release");
		}
		if let Some(profile) = self.profile {
			full_message.push_str(&format!(" --profile {}", profile));
		}
		if let Some(features) = &self.features {
			full_message.push_str(&format!(" --features {}", features));
		}
		#[cfg(feature = "chain")]
		{
			if self.benchmark {
				full_message.push_str(" --benchmark");
			}
			if self.try_runtime {
				full_message.push_str(" --try-runtime");
			}
			if self.deterministic {
				full_message.push_str(" --deterministic");
			}
			if let Some(tag) = &self.tag {
				full_message.push_str(&format!(" --tag {}", tag));
			}
			if self.only_runtime {
				full_message.push_str(" --only-runtime");
			}
		}
		#[cfg(feature = "contract")]
		{
			if let Some(metadata) = &self.metadata {
				full_message.push_str(&format!(" --metadata {}", metadata));
			}
		}
		full_message
	}
}

/// Subcommand for building chain artifacts.
#[derive(Subcommand, Serialize)]
pub(crate) enum Command {
	/// Build a chain specification and its genesis artifacts.
	#[clap(alias = "s")]
	#[cfg(feature = "chain")]
	Spec(BuildSpecCommand),
}

#[cfg(feature = "chain")]
fn collect_features(input: &str, benchmark: bool, try_runtime: bool) -> Vec<&str> {
	let mut feature_list: Vec<&str> = input.split(',').filter(|s| !s.is_empty()).collect();
	if benchmark && !feature_list.contains(&Benchmark.as_ref()) {
		feature_list.push(Benchmark.as_ref());
	}
	if try_runtime && !feature_list.contains(&TryRuntime.as_ref()) {
		feature_list.push(TryRuntime.as_ref());
	}
	feature_list
}

fn effective_profile(args: &BuildArgs) -> Profile {
	args.profile.unwrap_or_else(|| args.release.into())
}

fn explicit_features(args: &BuildArgs) -> Vec<String> {
	let mut features: Vec<String> = args
		.features
		.as_deref()
		.unwrap_or_default()
		.split(',')
		.filter(|s| !s.trim().is_empty())
		.map(|s| s.trim().to_string())
		.collect();
	features.sort();
	features.dedup();
	features
}

#[cfg(feature = "chain")]
fn chain_features(args: &BuildArgs) -> Vec<String> {
	let mut features = collect_features(
		args.features.as_deref().unwrap_or_default(),
		args.benchmark,
		args.try_runtime,
	)
	.into_iter()
	.map(str::to_string)
	.collect::<Vec<_>>();
	features.sort();
	features.dedup();
	features
}

fn combined_subprocess_output(output: &ProcessOutput) -> String {
	let mut combined = String::new();
	let stdout = String::from_utf8_lossy(&output.stdout);
	let stderr = String::from_utf8_lossy(&output.stderr);
	if !stdout.is_empty() {
		combined.push_str(&stdout);
	}
	if !stderr.is_empty() {
		combined.push_str(&stderr);
	}
	combined
}

fn map_json_build_error(err: anyhow::Error, prompt_hint: &str) -> anyhow::Error {
	let message = err.to_string();
	if err.downcast_ref::<PromptRequiredError>().is_some() ||
		err.downcast_ref::<BuildCommandError>().is_some()
	{
		return err;
	}
	if message.contains(JSON_PROMPT_ERR) {
		return PromptRequiredError(prompt_hint.to_string()).into();
	}
	BuildCommandError::new("Build failed").with_details(message).into()
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: &BuildArgs) -> anyhow::Result<()> {
		// If only contract feature enabled, build as contract
		let project_path =
			crate::common::builds::ensure_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(&project_path)? {
			let build_mode = contract::resolve_build_mode(args);
			if let BuildMode::Verifiable = build_mode {
				Docker::ensure_running().await?;
			}
			let image = contract::resolve_image(args)?;
			BuildContract { path: project_path, build_mode, metadata: args.metadata, image }
				.execute()?;
			return Ok(());
		}

		// If project is a parachain runtime, build as parachain runtime
		#[cfg(feature = "chain")]
		if args.only_runtime ||
			args.deterministic ||
			pop_chains::runtime::is_supported(&project_path)
		{
			if args.deterministic {
				Docker::ensure_running().await?;
			}
			let profile = effective_profile(args);
			let features = args.features.clone().unwrap_or_default();
			let mut feature_list = collect_features(&features, args.benchmark, args.try_runtime);
			feature_list.sort();

			let runtime_path =
				crate::common::builds::find_runtime_dir(&project_path, &mut cli::Cli)?;

			BuildRuntime {
				path: runtime_path,
				profile,
				benchmark: feature_list.contains(&Benchmark.as_ref()),
				try_runtime: feature_list.contains(&TryRuntime.as_ref()),
				deterministic: args.deterministic,
				features: feature_list.into_iter().map(|f| f.to_string()).collect(),
				tag: args.tag.clone(),
			}
			.execute()
			.await?;
			cli::Cli.info(args.display())?;
			return Ok(());
		}

		// If project is a parachain runtime, build as parachain runtime
		#[cfg(feature = "chain")]
		if pop_chains::is_supported(&project_path) {
			let profile = effective_profile(args);
			let features = args.features.clone().unwrap_or_default();
			let feature_list = collect_features(&features, args.benchmark, args.try_runtime);

			BuildChain {
				path: project_path,
				package: args.package.clone(),
				profile,
				benchmark: feature_list.contains(&Benchmark.as_ref()),
				try_runtime: feature_list.contains(&TryRuntime.as_ref()),
				features: feature_list.into_iter().map(|f| f.to_string()).collect(),
			}
			.execute()?;
			cli::Cli.info(args.display())?;
			return Ok(());
		}

		// Otherwise build as a normal Rust project
		Self::build(args, &project_path, &mut cli::Cli)?;
		cli::Cli.info(args.display())?;
		Ok(())
	}

	/// Executes the command in JSON mode.
	pub(crate) async fn execute_json(args: &BuildArgs) -> anyhow::Result<()> {
		let project_path =
			crate::common::builds::ensure_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(&project_path)? {
			let build_mode = contract::resolve_build_mode(args);
			if let BuildMode::Verifiable = build_mode {
				Docker::ensure_running().await?;
			}
			let image = contract::resolve_image(args)?;
			let artifact_path =
				BuildContract { path: project_path, build_mode, metadata: args.metadata, image }
					.execute_json()
					.map_err(|e| {
						map_json_build_error(e, "--path must point to a valid contract project")
					})?;
			let profile = match build_mode {
				BuildMode::Debug => "debug",
				BuildMode::Release => "release",
				BuildMode::Verifiable => "verifiable",
			}
			.to_string();
			CliResponse::ok(BuildOutput {
				artifact_path: artifact_path.display().to_string(),
				profile,
				features: explicit_features(args),
			})
			.print_json();
			return Ok(());
		}

		#[cfg(feature = "chain")]
		if args.only_runtime ||
			args.deterministic ||
			pop_chains::runtime::is_supported(&project_path)
		{
			if args.deterministic {
				Docker::ensure_running().await?;
			}
			let profile = effective_profile(args);
			let features = chain_features(args);
			let mut json_cli = crate::cli::JsonCli;
			let runtime_path =
				crate::common::builds::find_runtime_dir(&project_path, &mut json_cli).map_err(
					|e| {
						map_json_build_error(
							e,
							"`build --json` requires explicit runtime selection when multiple runtimes are available",
						)
					},
				)?;
			let artifact_path = BuildRuntime {
				path: runtime_path,
				profile,
				benchmark: features.iter().any(|f| f == Benchmark.as_ref()),
				try_runtime: features.iter().any(|f| f == TryRuntime.as_ref()),
				deterministic: args.deterministic,
				features: features.clone(),
				tag: args.tag.clone(),
			}
			.execute_json()
			.await
			.map_err(|e| map_json_build_error(e, "build runtime prerequisites must be provided"))?;
			CliResponse::ok(BuildOutput {
				artifact_path: artifact_path.display().to_string(),
				profile: profile.to_string(),
				features,
			})
			.print_json();
			return Ok(());
		}

		#[cfg(feature = "chain")]
		if pop_chains::is_supported(&project_path) {
			let profile = effective_profile(args);
			let features = chain_features(args);
			let artifact_path = BuildChain {
				path: project_path,
				package: args.package.clone(),
				profile,
				benchmark: features.iter().any(|f| f == Benchmark.as_ref()),
				try_runtime: features.iter().any(|f| f == TryRuntime.as_ref()),
				features: features.clone(),
			}
			.execute_json()
			.map_err(|e| map_json_build_error(e, "build chain prerequisites must be provided"))?;
			CliResponse::ok(BuildOutput {
				artifact_path: artifact_path.display().to_string(),
				profile: profile.to_string(),
				features,
			})
			.print_json();
			return Ok(());
		}

		let output = Self::build_json(args, &project_path)
			.map_err(|e| map_json_build_error(e, "build prerequisites must be provided"))?;
		CliResponse::ok(output).print_json();
		Ok(())
	}

	/// Builds a Rust project.
	///
	/// # Arguments
	/// * `path` - The path to the project.
	/// * `package` - A specific package to be built.
	/// * `release` - Whether the release profile is to be used.
	fn build(args: &BuildArgs, path: &Path, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let project = if args.package.is_some() { PACKAGE } else { PROJECT };
		cli.intro(format!("Building your {project}"))?;

		let mut cargo_args = vec!["build"];
		if let Some(package) = args.package.as_deref() {
			cargo_args.push("--package");
			cargo_args.push(package)
		}
		let profile = effective_profile(args);
		if profile == Profile::Release {
			cargo_args.push("--release");
		} else if profile == Profile::Production {
			cargo_args.push("--profile=production");
		}

		let feature_input = args.features.clone().unwrap_or_default();
		#[allow(unused_mut)]
		let mut features: Vec<&str> = feature_input.split(',').filter(|s| !s.is_empty()).collect();
		#[cfg(feature = "chain")]
		if args.benchmark && !features.contains(&Benchmark.as_ref()) {
			features.push(Benchmark.as_ref());
		}
		#[cfg(feature = "chain")]
		if args.try_runtime && !features.contains(&TryRuntime.as_ref()) {
			features.push(TryRuntime.as_ref());
		}
		let feature_arg = format!("--features={}", features.join(","));
		if !features.is_empty() {
			cargo_args.push(&feature_arg);
		}

		// Pre-fetch dependencies so users see download progress before compilation begins.
		cmd("cargo", ["fetch"]).dir(path).stdout_null().run()?;

		cmd("cargo", cargo_args).dir(path).run()?;

		cli.info(format!(
			"The {project} was built in {profile} mode{}.",
			features
				.is_empty()
				.then(String::new)
				.unwrap_or_else(|| format!(" with the following features: {}", features.join(",")))
		))?;
		cli.outro("Build completed successfully!")?;
		Ok(())
	}

	fn build_json(args: &BuildArgs, path: &Path) -> anyhow::Result<BuildOutput> {
		let mut cargo_args = vec!["build"];
		if let Some(package) = args.package.as_deref() {
			cargo_args.push("--package");
			cargo_args.push(package);
		}
		let profile = effective_profile(args);
		if profile == Profile::Release {
			cargo_args.push("--release");
		} else if profile == Profile::Production {
			cargo_args.push("--profile=production");
		}
		let mut features = explicit_features(args);
		#[cfg(feature = "chain")]
		{
			if args.benchmark && !features.iter().any(|f| f == Benchmark.as_ref()) {
				features.push(Benchmark.as_ref().to_string());
			}
			if args.try_runtime && !features.iter().any(|f| f == TryRuntime.as_ref()) {
				features.push(TryRuntime.as_ref().to_string());
			}
			features.sort();
			features.dedup();
		}
		let feature_arg = format!("--features={}", features.join(","));
		if !features.is_empty() {
			cargo_args.push(&feature_arg);
		}

		cmd("cargo", ["fetch"]).dir(path).stdout_null().run()?;
		let output = cmd("cargo", cargo_args)
			.dir(path)
			.stdout_capture()
			.stderr_capture()
			.unchecked()
			.run()?;
		let combined = combined_subprocess_output(&output);
		if !combined.is_empty() {
			eprint!("{combined}");
			if !combined.ends_with('\n') {
				eprintln!();
			}
		}
		if !output.status.success() {
			let details =
				if combined.is_empty() { "cargo build failed".to_string() } else { combined };
			return Err(BuildCommandError::new("Build failed").with_details(details).into());
		}

		let artifact_path = profile.target_directory(path);
		Ok(BuildOutput {
			artifact_path: artifact_path.display().to_string(),
			profile: profile.to_string(),
			features,
		})
	}
}

#[cfg(feature = "chain")]
impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			Command::Spec(_) => write!(f, "spec"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::output::BuildCommandError;
	use cli::MockCli;
	#[cfg(feature = "chain")]
	use pop_common::manifest::add_feature;
	use pop_common::manifest::add_production_profile;
	use std::path::Path;

	#[test]
	fn test_build_args_display() {
		let args = BuildArgs {
			command: None,
			path: Some(PathBuf::from("my-path")),
			path_pos: None,
			package: Some("my-package".to_string()),
			release: true,
			profile: None,
			features: Some("feature1,feature2".to_string()),
			#[cfg(feature = "chain")]
			benchmark: true,
			#[cfg(feature = "chain")]
			try_runtime: true,
			#[cfg(feature = "chain")]
			deterministic: true,
			#[cfg(feature = "chain")]
			tag: Some("v1".to_string()),
			#[cfg(feature = "chain")]
			only_runtime: true,
			#[cfg(feature = "contract")]
			metadata: Some(MetadataSpec::Solidity),
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		};
		let expected =
			"pop build --path my-path --package my-package --release --features feature1,feature2";
		let mut expected = expected.to_string();
		#[cfg(feature = "chain")]
		{
			expected.push_str(" --benchmark --try-runtime --deterministic --tag v1 --only-runtime");
		}
		#[cfg(feature = "contract")]
		{
			expected.push_str(" --metadata solidity");
		}
		assert_eq!(args.display(), expected);

		let args = BuildArgs {
			command: None,
			path: None,
			path_pos: Some(PathBuf::from("my-path-pos")),
			package: None,
			release: false,
			profile: Some(Profile::Debug),
			features: None,
			#[cfg(feature = "chain")]
			benchmark: false,
			#[cfg(feature = "chain")]
			try_runtime: false,
			#[cfg(feature = "chain")]
			deterministic: false,
			#[cfg(feature = "chain")]
			tag: None,
			#[cfg(feature = "chain")]
			only_runtime: false,
			#[cfg(feature = "contract")]
			metadata: None,
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		};
		assert_eq!(args.display(), "pop build my-path-pos --profile debug");
	}

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		#[cfg(feature = "chain")]
		let benchmark = Benchmark.as_ref();
		#[cfg(feature = "chain")]
		let try_runtime = TryRuntime.as_ref();
		#[cfg(feature = "chain")]
		let features = vec![benchmark, try_runtime];
		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;
		add_production_profile(&project_path)?;
		#[cfg(feature = "chain")]
		for feature in &features {
			add_feature(&project_path, (feature.to_string(), vec![]))?;
		}
		for package in [None, Some(name.to_string())] {
			// Each (release, profile) pair produces an effective profile.
			// When release=true the profile is always Release, so iterating over all
			// Profile::VARIANTS with release=true just repeats the same build.
			// Use representative combinations to avoid redundant `cargo build` calls.
			for (release, profile) in
				[(false, Profile::Debug), (true, Profile::Release), (false, Profile::Production)]
			{
				#[allow(unused_variables)]
				for &(benchmark_flag, try_runtime_flag, features_flag, expected_features) in &[
					// No features
					(false, false, &vec![], &vec![]),
					// --features runtime-benchmarks
					#[cfg(feature = "chain")]
					(false, false, &vec![benchmark], &vec![benchmark]),
					// --benchmark
					#[cfg(feature = "chain")]
					(true, false, &vec![], &vec![benchmark]),
					// --features try-runtime
					#[cfg(feature = "chain")]
					(false, false, &vec![try_runtime], &vec![try_runtime]),
					// --try-runtime
					#[cfg(feature = "chain")]
					(false, true, &vec![], &vec![try_runtime]),
					// --features runtime-benchmarks,try-runtime
					#[cfg(feature = "chain")]
					(false, false, &features, &features),
					#[cfg(feature = "chain")]
					// --benchmark --try-runtime
					(true, true, &vec![], &features),
				] {
					test_build(
						package.clone(),
						&project_path,
						&profile,
						release,
						#[cfg(feature = "chain")]
						benchmark_flag,
						#[cfg(feature = "chain")]
						try_runtime_flag,
						#[cfg(feature = "chain")]
						false,
						features_flag,
						expected_features,
					)?;
				}
			}
		}
		Ok(())
	}

	fn test_build(
		package: Option<String>,
		project_path: &Path,
		profile: &Profile,
		release: bool,
		#[cfg(feature = "chain")] benchmark: bool,
		#[cfg(feature = "chain")] try_runtime: bool,
		#[cfg(feature = "chain")] deterministic: bool,
		features: &Vec<&str>,
		expected_features: &Vec<&str>,
	) -> anyhow::Result<()> {
		let project = if package.is_some() { PACKAGE } else { PROJECT };
		let mut cli = MockCli::new().expect_intro(format!("Building your {project}"));
		cli = if expected_features.is_empty() {
			cli.expect_info(format!("The {project} was built in {profile} mode."))
		} else {
			cli.expect_info(format!(
				"The {project} was built in {profile} mode with the following features: {}.",
				expected_features.join(",")
			))
		};
		cli = cli.expect_outro("Build completed successfully!");
		assert!(
			Command::build(
				&BuildArgs {
					#[cfg(feature = "chain")]
					command: None,
					path: Some(project_path.to_path_buf()),
					path_pos: Some(project_path.to_path_buf()),
					package: package.clone(),
					release,
					profile: Some(*profile),
					#[cfg(feature = "chain")]
					benchmark,
					#[cfg(feature = "chain")]
					try_runtime,
					#[cfg(feature = "chain")]
					deterministic,
					#[cfg(feature = "chain")]
					tag: None,
					features: Some(features.join(",")),
					#[cfg(feature = "chain")]
					only_runtime: false,
					#[cfg(feature = "contract")]
					metadata: None,
					#[cfg(feature = "contract")]
					verifiable: false,
					#[cfg(feature = "contract")]
					image: None,
				},
				project_path,
				&mut cli,
			)
			.is_ok()
		);
		cli.verify()
	}

	#[test]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::Spec(Default::default()).to_string(), "spec");
	}

	#[test]
	#[cfg(feature = "chain")]
	fn collect_features_works() {
		assert_eq!(
			collect_features("runtime-benchmarks", false, false),
			vec!["runtime-benchmarks"]
		);
		assert_eq!(collect_features("try-runtime", false, false), vec!["try-runtime"]);
		assert_eq!(
			collect_features("try-runtime", true, false),
			vec!["try-runtime", "runtime-benchmarks"]
		);
		assert_eq!(
			collect_features("runtime-benchmarks", false, true),
			vec!["runtime-benchmarks", "try-runtime"]
		);
		assert_eq!(
			collect_features("runtime-benchmarks,try-runtime", false, false),
			vec!["runtime-benchmarks", "try-runtime"]
		);
		assert_eq!(
			collect_features("runtime-benchmarks,try-runtime", true, true),
			vec!["runtime-benchmarks", "try-runtime"]
		);
	}

	#[tokio::test]
	async fn execute_works_with_basic_option() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;

		Command::execute(&BuildArgs {
			#[cfg(feature = "chain")]
			command: None,
			path: Some(project_path.clone()),
			path_pos: None,
			package: None,
			release: false,
			profile: None,
			features: None,
			#[cfg(feature = "chain")]
			benchmark: false,
			#[cfg(feature = "chain")]
			try_runtime: false,
			#[cfg(feature = "chain")]
			deterministic: false,
			#[cfg(feature = "chain")]
			tag: None,
			#[cfg(feature = "chain")]
			only_runtime: false,
			#[cfg(feature = "contract")]
			metadata: None,
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		})
		.await?;

		Ok(())
	}

	#[tokio::test]
	async fn execute_works_with_advanced_options() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);

		// Create a binary project
		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;

		// Add production profile to Cargo.toml
		add_production_profile(&project_path)?;

		// Add some custom features to test with
		#[cfg(feature = "chain")]
		{
			add_feature(&project_path, ("runtime-benchmarks".to_string(), vec![]))?;
			add_feature(&project_path, ("try-runtime".to_string(), vec![]))?;
		}

		// Test 1: Execute with release mode
		Command::execute(&BuildArgs {
			#[cfg(feature = "chain")]
			command: None,
			path: Some(project_path.clone()),
			path_pos: None,
			package: None,
			release: true,
			profile: None,
			features: None,
			#[cfg(feature = "chain")]
			benchmark: false,
			#[cfg(feature = "chain")]
			try_runtime: false,
			#[cfg(feature = "chain")]
			deterministic: false,
			#[cfg(feature = "chain")]
			tag: None,
			#[cfg(feature = "chain")]
			only_runtime: false,
			#[cfg(feature = "contract")]
			metadata: None,
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		})
		.await?;

		// Test 2: Execute with production profile
		Command::execute(&BuildArgs {
			#[cfg(feature = "chain")]
			command: None,
			path: Some(project_path.clone()),
			path_pos: None,
			package: None,
			release: false,
			profile: Some(Profile::Production),
			features: None,
			#[cfg(feature = "chain")]
			benchmark: false,
			#[cfg(feature = "chain")]
			try_runtime: false,
			#[cfg(feature = "chain")]
			deterministic: false,
			#[cfg(feature = "chain")]
			tag: None,
			#[cfg(feature = "chain")]
			only_runtime: false,
			#[cfg(feature = "contract")]
			metadata: None,
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		})
		.await?;

		// Test 3: Execute with custom features
		#[cfg(feature = "chain")]
		{
			Command::execute(&BuildArgs {
				command: None,
				path: Some(project_path.clone()),
				path_pos: None,
				package: None,
				release: false,
				profile: None,
				features: Some("runtime-benchmarks,try-runtime".to_string()),
				benchmark: false,
				try_runtime: false,
				deterministic: false,
				tag: None,
				only_runtime: false,
				#[cfg(feature = "contract")]
				metadata: None,
				#[cfg(feature = "contract")]
				verifiable: false,
				#[cfg(feature = "contract")]
				image: None,
			})
			.await?;
		}

		// Test 4: Execute with package parameter
		Command::execute(&BuildArgs {
			#[cfg(feature = "chain")]
			command: None,
			path: Some(project_path.clone()),
			path_pos: None,
			package: Some(name.to_string()),
			release: true,
			profile: Some(Profile::Release),
			features: None,
			#[cfg(feature = "chain")]
			benchmark: false,
			#[cfg(feature = "chain")]
			try_runtime: false,
			#[cfg(feature = "chain")]
			deterministic: false,
			#[cfg(feature = "chain")]
			tag: None,
			#[cfg(feature = "chain")]
			only_runtime: false,
			#[cfg(feature = "contract")]
			metadata: None,
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		})
		.await?;

		// Test 5: Execute with path_pos instead of path
		Command::execute(&BuildArgs {
			#[cfg(feature = "chain")]
			command: None,
			path: None,
			path_pos: Some(project_path.clone()),
			package: None,
			release: false,
			profile: Some(Profile::Debug),
			features: None,
			#[cfg(feature = "chain")]
			benchmark: false,
			#[cfg(feature = "chain")]
			try_runtime: false,
			#[cfg(feature = "chain")]
			deterministic: false,
			#[cfg(feature = "chain")]
			tag: None,
			#[cfg(feature = "chain")]
			only_runtime: false,
			#[cfg(feature = "contract")]
			metadata: None,
			#[cfg(feature = "contract")]
			verifiable: false,
			#[cfg(feature = "contract")]
			image: None,
		})
		.await?;

		// Test 6: Execute with benchmark and try_runtime flags
		#[cfg(feature = "chain")]
		{
			Command::execute(&BuildArgs {
				command: None,
				path: Some(project_path.clone()),
				path_pos: None,
				package: None,
				release: true,
				profile: None,
				features: None,
				benchmark: true,
				try_runtime: true,
				deterministic: false,
				tag: None,
				only_runtime: false,
				#[cfg(feature = "contract")]
				metadata: None,
				#[cfg(feature = "contract")]
				verifiable: false,
				#[cfg(feature = "contract")]
				image: None,
			})
			.await?;
		}

		Ok(())
	}

	#[test]
	fn build_json_returns_structured_output() -> anyhow::Result<()> {
		let name = "json_build_ok";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;
		std::fs::write(
			project_path.join("src/main.rs"),
			"//! test binary\nfn main() { println!(\"ok\"); }\n",
		)?;

		let output = Command::build_json(
			&BuildArgs {
				#[cfg(feature = "chain")]
				command: None,
				path: Some(project_path.clone()),
				path_pos: None,
				package: Some(name.to_string()),
				release: false,
				profile: Some(Profile::Debug),
				features: None,
				#[cfg(feature = "chain")]
				benchmark: false,
				#[cfg(feature = "chain")]
				try_runtime: false,
				#[cfg(feature = "chain")]
				deterministic: false,
				#[cfg(feature = "chain")]
				tag: None,
				#[cfg(feature = "chain")]
				only_runtime: false,
				#[cfg(feature = "contract")]
				metadata: None,
				#[cfg(feature = "contract")]
				verifiable: false,
				#[cfg(feature = "contract")]
				image: None,
			},
			&project_path,
		)?;
		assert_eq!(output.profile, "debug");
		assert!(output.artifact_path.contains("target/debug"));
		assert_eq!(output.features, Vec::<String>::new());
		Ok(())
	}

	#[test]
	fn build_json_package_library_returns_existing_profile_directory() -> anyhow::Result<()> {
		let name = "json_build_lib";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		cmd("cargo", ["new", name, "--lib"]).dir(path).run()?;
		std::fs::write(
			project_path.join("src/lib.rs"),
			"//! test library\n\n/// Returns the sum of two numbers.\npub fn add(left: u64, right: u64) -> u64 {\n\tleft + right\n}\n",
		)?;

		let output = Command::build_json(
			&BuildArgs {
				#[cfg(feature = "chain")]
				command: None,
				path: Some(project_path.clone()),
				path_pos: None,
				package: Some(name.to_string()),
				release: false,
				profile: Some(Profile::Debug),
				features: None,
				#[cfg(feature = "chain")]
				benchmark: false,
				#[cfg(feature = "chain")]
				try_runtime: false,
				#[cfg(feature = "chain")]
				deterministic: false,
				#[cfg(feature = "chain")]
				tag: None,
				#[cfg(feature = "chain")]
				only_runtime: false,
				#[cfg(feature = "contract")]
				metadata: None,
				#[cfg(feature = "contract")]
				verifiable: false,
				#[cfg(feature = "contract")]
				image: None,
			},
			&project_path,
		)?;
		let artifact_path = PathBuf::from(&output.artifact_path);
		assert_eq!(artifact_path, Profile::Debug.target_directory(&project_path));
		assert!(artifact_path.exists());
		assert!(artifact_path.is_dir());
		assert_eq!(output.profile, "debug");
		assert_eq!(output.features, Vec::<String>::new());
		Ok(())
	}

	#[test]
	fn build_json_failure_returns_build_command_error() -> anyhow::Result<()> {
		let name = "json_build_fail";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;
		std::fs::write(project_path.join("src/main.rs"), "fn main() { let _ = ; }\n")?;

		let err = Command::build_json(
			&BuildArgs {
				#[cfg(feature = "chain")]
				command: None,
				path: Some(project_path.clone()),
				path_pos: None,
				package: None,
				release: false,
				profile: Some(Profile::Debug),
				features: None,
				#[cfg(feature = "chain")]
				benchmark: false,
				#[cfg(feature = "chain")]
				try_runtime: false,
				#[cfg(feature = "chain")]
				deterministic: false,
				#[cfg(feature = "chain")]
				tag: None,
				#[cfg(feature = "chain")]
				only_runtime: false,
				#[cfg(feature = "contract")]
				metadata: None,
				#[cfg(feature = "contract")]
				verifiable: false,
				#[cfg(feature = "contract")]
				image: None,
			},
			&project_path,
		)
		.unwrap_err();
		let build_err = err.downcast_ref::<BuildCommandError>().expect("build error expected");
		assert_eq!(build_err.to_string(), "Build failed");
		assert!(build_err.details().is_some());
		Ok(())
	}
}
