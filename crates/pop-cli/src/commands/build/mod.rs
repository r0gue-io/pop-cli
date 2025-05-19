// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::Project::{self, *},
};
use clap::{Args, Subcommand};
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
use contract::BuildContract;
use duct::cmd;
use pop_common::Profile;
use std::path::PathBuf;
#[cfg(feature = "parachain")]
use {
	parachain::BuildParachain,
	runtime::{BuildRuntime, Feature::*},
	spec::BuildSpecCommand,
	std::fmt::{Display, Formatter, Result},
};

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod contract;
#[cfg(feature = "parachain")]
pub(crate) mod parachain;
#[cfg(feature = "parachain")]
pub(crate) mod runtime;
#[cfg(feature = "parachain")]
pub(crate) mod spec;

#[cfg(feature = "parachain")]
const CHAIN_HELP_HEADER: &str = "Chain options";
#[cfg(feature = "parachain")]
const RUNTIME_HELP_HEADER: &str = "Runtime options";
const PACKAGE: &str = "package";
#[cfg(feature = "parachain")]
const PARACHAIN: &str = "parachain";
const PROJECT: &str = "project";

/// Arguments for building a project.
#[derive(Args)]
#[cfg_attr(test, derive(Default))]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
	#[command(subcommand)]
	#[cfg(feature = "parachain")]
	pub command: Option<Command>,
	/// Directory path with flag for your project [default: current directory]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
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
	#[cfg(feature = "parachain")]
	pub(crate) benchmark: bool,
	/// For testing with `try-runtime`, always build with `try-runtime` feature.
	#[clap(short, long, help_heading = CHAIN_HELP_HEADER)]
	#[cfg(feature = "parachain")]
	pub(crate) try_runtime: bool,
	/// Whether to build a runtime deterministically.
	#[clap(short, long, help_heading = RUNTIME_HELP_HEADER)]
	#[cfg(feature = "parachain")]
	pub(crate) deterministic: bool,
	/// Whether to build only the runtime.
	#[clap(long, help_heading = RUNTIME_HELP_HEADER)]
	#[cfg(feature = "parachain")]
	pub(crate) only_runtime: bool,
}

/// Subcommand for building chain artifacts.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Build a chain specification and its genesis artifacts.
	#[clap(alias = "s")]
	#[cfg(feature = "parachain")]
	Spec(BuildSpecCommand),
}

#[cfg(feature = "parachain")]
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
	pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<Project> {
		#[cfg(any(
			feature = "polkavm-contracts",
			feature = "wasm-contracts",
			feature = "parachain"
		))]
		// If only contract feature enabled, build as contract
		let project_path =
			crate::common::builds::get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		if pop_contracts::is_supported(project_path.as_deref())? {
			// All commands originating from root command are valid
			let release = match args.profile {
				Some(profile) => profile.into(),
				None => args.release,
			};
			BuildContract { path: project_path, release }.execute()?;
			return Ok(Contract);
		}

		// If project is a parachain runtime, build as parachain runtime
		#[cfg(feature = "parachain")]
		if args.only_runtime || pop_parachains::runtime::is_supported(project_path.as_deref())? {
			let profile = match args.profile {
				Some(profile) => profile,
				None => args.release.into(),
			};
			let temp_path = PathBuf::from("./");
			let features = args.features.unwrap_or_default();
			let feature_list = collect_features(&features, args.benchmark, args.try_runtime);

			BuildRuntime {
				path: project_path.unwrap_or(temp_path).to_path_buf(),
				profile,
				benchmark: feature_list.contains(&Benchmark.as_ref()),
				try_runtime: feature_list.contains(&TryRuntime.as_ref()),
				deterministic: args.deterministic,
			}
			.execute()?;
			return Ok(Chain);
		}

		// If project is a parachain runtime, build as parachain runtime
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(project_path.as_deref())? {
			let profile = match args.profile {
				Some(profile) => profile,
				None => args.release.into(),
			};
			let temp_path = PathBuf::from("./");
			let features = args.features.unwrap_or_default();
			let feature_list = collect_features(&features, args.benchmark, args.try_runtime);

			BuildParachain {
				path: project_path.unwrap_or(temp_path).to_path_buf(),
				package: args.package,
				profile,
				benchmark: feature_list.contains(&Benchmark.as_ref()),
				try_runtime: feature_list.contains(&TryRuntime.as_ref()),
			}
			.execute()?;
			return Ok(Chain);
		}

		// Otherwise build as a normal Rust project
		Self::build(args, &mut Cli).map(|_| Unknown)
	}

	/// Builds a Rust project.
	///
	/// # Arguments
	/// * `path` - The path to the project.
	/// * `package` - A specific package to be built.
	/// * `release` - Whether the release profile is to be used.
	fn build(args: BuildArgs, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let project = if args.package.is_some() { PACKAGE } else { PROJECT };
		cli.intro(format!("Building your {project}"))?;

		let mut _args = vec!["build"];
		if let Some(package) = args.package.as_deref() {
			_args.push("--package");
			_args.push(package)
		}
		let profile = args.profile.unwrap_or(Profile::Debug);
		if profile == Profile::Release {
			_args.push("--release");
		} else if profile == Profile::Production {
			_args.push("--profile=production");
		}

		let feature_input = args.features.unwrap_or_default();
		#[allow(unused_mut)]
		let mut features: Vec<&str> = feature_input.split(',').filter(|s| !s.is_empty()).collect();
		#[cfg(feature = "parachain")]
		if args.benchmark && !features.contains(&Benchmark.as_ref()) {
			features.push(Benchmark.as_ref());
		}
		#[cfg(feature = "parachain")]
		if args.try_runtime && !features.contains(&TryRuntime.as_ref()) {
			features.push(TryRuntime.as_ref());
		}
		let feature_arg = format!("--features={}", features.join(","));
		if !features.is_empty() {
			_args.push(&feature_arg);
		}

		cmd("cargo", _args).dir(args.path.unwrap_or_else(|| "./".into())).run()?;

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

#[cfg(feature = "parachain")]
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
	#[cfg(feature = "parachain")]
	use pop_common::manifest::add_feature;
	use pop_common::manifest::add_production_profile;
	use strum::VariantArray;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		#[cfg(feature = "parachain")]
		let benchmark = Benchmark.as_ref();
		#[cfg(feature = "parachain")]
		let try_runtime = TryRuntime.as_ref();
		#[cfg(feature = "parachain")]
		let features = vec![benchmark, try_runtime];
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		add_production_profile(&project_path)?;
		#[cfg(feature = "parachain")]
		for feature in features.to_vec() {
			add_feature(&project_path, (feature.to_string(), vec![]))?;
		}
		for package in [None, Some(name.to_string())] {
			for release in [true, false] {
				for profile in Profile::VARIANTS {
					let profile = if release { Profile::Release } else { profile.clone() };
					#[allow(unused_variables)]
					for &(benchmark_flag, try_runtime_flag, features_flag, expected_features) in &[
						// No features
						(false, false, &vec![], &vec![]),
						// --features runtime-benchmarks
						#[cfg(feature = "parachain")]
						(false, false, &vec![benchmark], &vec![benchmark]),
						// --benchmark
						#[cfg(feature = "parachain")]
						(true, false, &vec![], &vec![benchmark]),
						// --features try-runtime
						#[cfg(feature = "parachain")]
						(false, false, &vec![try_runtime], &vec![try_runtime]),
						// --try-runtime
						#[cfg(feature = "parachain")]
						(false, true, &vec![], &vec![try_runtime]),
						// --features runtime-benchmarks,try-runtime
						#[cfg(feature = "parachain")]
						(false, false, &features, &features),
						#[cfg(feature = "parachain")]
						// --benchmark --try-runtime
						(true, true, &vec![], &features),
					] {
						test_build(
							package.clone(),
							&project_path,
							&profile,
							release,
							#[cfg(feature = "parachain")]
							benchmark_flag,
							#[cfg(feature = "parachain")]
							try_runtime_flag,
							#[cfg(feature = "parachain")]
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
		project_path: &PathBuf,
		profile: &Profile,
		release: bool,
		#[cfg(feature = "parachain")] benchmark: bool,
		#[cfg(feature = "parachain")] try_runtime: bool,
		#[cfg(feature = "parachain")] deterministic: bool,
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
		assert!(Command::build(
			BuildArgs {
				#[cfg(feature = "parachain")]
				command: None,
				path: Some(project_path.clone()),
				path_pos: Some(project_path.clone()),
				package: package.clone(),
				release,
				profile: Some(profile.clone()),
				#[cfg(feature = "parachain")]
				benchmark,
				#[cfg(feature = "parachain")]
				try_runtime,
				#[cfg(feature = "parachain")]
				deterministic,
				features: Some(features.join(",")),
				#[cfg(feature = "parachain")]
				only_runtime: false
			},
			&mut cli,
		)
		.is_ok());
		cli.verify()
	}

	#[test]
	fn command_display_works() {
		#[cfg(feature = "parachain")]
		assert_eq!(Command::Spec(Default::default()).to_string(), "spec");
	}

	#[test]
	#[cfg(feature = "parachain")]
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
}
