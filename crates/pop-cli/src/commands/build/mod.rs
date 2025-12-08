// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, Cli};
use clap::{Args, Subcommand};
use duct::cmd;
use pop_common::Profile;
use serde::Serialize;
use std::path::{Path, PathBuf};
#[cfg(feature = "chain")]
use {
	chain::BuildChain,
	pop_common::Docker,
	runtime::{BuildRuntime, Feature::*},
	spec::BuildSpecCommand,
	std::fmt::{Display, Formatter, Result},
};
#[cfg(feature = "contract")]
use {contract::BuildContract, pop_contracts::MetadataSpec};

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
	let mut feature_list: Vec<&str> = input.split(",").collect();
	if benchmark && !feature_list.contains(&Benchmark.as_ref()) {
		feature_list.push(Benchmark.as_ref());
	}
	if try_runtime && !feature_list.contains(&TryRuntime.as_ref()) {
		feature_list.push(TryRuntime.as_ref());
	}
	feature_list
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: &BuildArgs) -> anyhow::Result<()> {
		// If only contract feature enabled, build as contract
		let project_path =
			crate::common::builds::ensure_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(&project_path)? {
			// All commands originating from root command are valid
			let release = match &args.profile {
				Some(profile) => (*profile).into(),
				None => args.release,
			};
			BuildContract { path: project_path, release, metadata: args.metadata }.execute()?;
			return Ok(());
		}

		// If project is a parachain runtime, build as parachain runtime
		#[cfg(feature = "chain")]
		if args.only_runtime ||
			args.deterministic ||
			pop_chains::runtime::is_supported(&project_path)
		{
			if args.deterministic {
				Docker::ensure_running()?;
			}
			let profile = match &args.profile {
				Some(profile) => *profile,
				None => args.release.into(),
			};
			let features = args.features.clone().unwrap_or_default();
			let mut feature_list = collect_features(&features, args.benchmark, args.try_runtime);
			feature_list.sort();

			let runtime_path = crate::common::builds::find_runtime_dir(&project_path, &mut Cli)?;

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
			return Ok(());
		}

		// If project is a parachain runtime, build as parachain runtime
		#[cfg(feature = "chain")]
		if pop_chains::is_supported(&project_path) {
			let profile = match &args.profile {
				Some(profile) => *profile,
				None => args.release.into(),
			};
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
			return Ok(());
		}

		// Otherwise build as a normal Rust project
		Self::build(args, &project_path, &mut Cli)
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
		let profile = args.profile.unwrap_or(Profile::Debug);
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
	use cli::MockCli;
	#[cfg(feature = "chain")]
	use pop_common::manifest::add_feature;
	use pop_common::manifest::add_production_profile;
	use std::path::Path;
	use strum::VariantArray;

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
			for release in [true, false] {
				for profile in Profile::VARIANTS {
					let profile = if release { Profile::Release } else { *profile };
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
			})
			.await?;
		}

		Ok(())
	}
}
