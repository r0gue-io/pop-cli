// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::builds::get_project_path,
};
use clap::{Args, Subcommand};
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
use contract::BuildContract;
use duct::cmd;
use pop_common::Profile;
use std::path::PathBuf;
#[cfg(feature = "parachain")]
use {parachain::BuildParachain, spec::BuildSpecCommand};

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod contract;
#[cfg(feature = "parachain")]
pub(crate) mod parachain;
#[cfg(feature = "parachain")]
pub(crate) mod spec;

const CHAIN_HELP_HEADER: &str = "Chain options";
const PACKAGE: &str = "package";
const PARACHAIN: &str = "parachain";
const PROJECT: &str = "project";
// Features.
const RUNTIME_BENCHMARKS_FEATURE: &str = "runtime-benchmarks";
const TRY_RUNTIME_FEATURE: &str = "try-runtime";

/// Arguments for building a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
	#[command(subcommand)]
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
	pub(crate) benchmark: bool,
	/// For testing with `try-runtime`, always build with `try-runtime` feature.
	#[clap(short, long, help_heading = CHAIN_HELP_HEADER)]
	pub(crate) try_runtime: bool,
}

/// Subcommand for building chain artifacts.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Build a chain specification and its genesis artifacts.
	#[cfg(feature = "parachain")]
	#[clap(alias = "s")]
	Spec(BuildSpecCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<&'static str> {
		// If only contract feature enabled, build as contract
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		if pop_contracts::is_supported(project_path.as_deref())? {
			// All commands originating from root command are valid
			let release = match args.profile {
				Some(profile) => profile.into(),
				None => args.release,
			};
			BuildContract { path: project_path, release }.execute()?;
			return Ok("contract");
		}

		// If only parachain feature enabled, build as parachain
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(project_path.as_deref())? {
			let profile = match args.profile {
				Some(profile) => profile,
				None => args.release.into(),
			};
			let temp_path = PathBuf::from("./");
			let features = args.features.unwrap_or_default();
			let mut feature_list: Vec<&str> = features.split(",").collect();

			if args.benchmark && !feature_list.contains(&RUNTIME_BENCHMARKS_FEATURE) {
				feature_list.push(RUNTIME_BENCHMARKS_FEATURE);
			}
			if args.try_runtime && !feature_list.contains(&TRY_RUNTIME_FEATURE) {
				feature_list.push(TRY_RUNTIME_FEATURE);
			}

			BuildParachain {
				path: project_path.unwrap_or(temp_path).to_path_buf(),
				package: args.package,
				profile,
				benchmark: feature_list.contains(&RUNTIME_BENCHMARKS_FEATURE),
				try_runtime: feature_list.contains(&TRY_RUNTIME_FEATURE),
			}
			.execute()?;
			return Ok("parachain");
		}

		// Otherwise build as a normal Rust project
		Self::build(args, &mut Cli)
	}

	/// Builds a Rust project.
	///
	/// # Arguments
	/// * `path` - The path to the project.
	/// * `package` - A specific package to be built.
	/// * `release` - Whether the release profile is to be used.
	fn build(args: BuildArgs, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
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
		let mut features: Vec<&str> = feature_input.split(',').filter(|s| !s.is_empty()).collect();
		if args.benchmark && !features.contains(&RUNTIME_BENCHMARKS_FEATURE) {
			features.push(RUNTIME_BENCHMARKS_FEATURE);
		}
		if args.try_runtime && !features.contains(&TRY_RUNTIME_FEATURE) {
			features.push(TRY_RUNTIME_FEATURE);
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
		Ok(project)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;
	use pop_common::manifest::{add_feature, add_production_profile};
	use strum::VariantArray;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		let benchmark = RUNTIME_BENCHMARKS_FEATURE;
		let try_runtime = TRY_RUNTIME_FEATURE;
		let features = vec![benchmark, try_runtime];
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		add_production_profile(&project_path)?;
		for feature in features.to_vec() {
			add_feature(&project_path, (feature.to_string(), vec![]))?;
		}
		for package in [None, Some(name.to_string())] {
			for release in [true, false] {
				for profile in Profile::VARIANTS {
					let profile = if release { Profile::Release } else { profile.clone() };
					for &(benchmark_flag, try_runtime_flag, features_flag, expected_features) in &[
						// No features
						(false, false, &vec![], &vec![]),
						// --features runtime-benchmarks
						(false, false, &vec![benchmark], &vec![benchmark]),
						// --benchmark
						(true, false, &vec![], &vec![benchmark]),
						// --features try-runtime
						(false, false, &vec![try_runtime], &vec![try_runtime]),
						// --try-runtime
						(false, true, &vec![], &vec![try_runtime]),
						// --features runtime-benchmarks,try-runtime
						(false, false, &features, &features),
						// --benchmark --try-runtime
						(true, true, &vec![], &features),
					] {
						test_build(
							package.clone(),
							&project_path,
							&profile,
							release,
							benchmark_flag,
							try_runtime_flag,
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
		benchmark: bool,
		try_runtime: bool,
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
		assert_eq!(
			Command::build(
				BuildArgs {
					command: None,
					path: Some(project_path.clone()),
					path_pos: Some(project_path.clone()),
					package: package.clone(),
					release,
					profile: Some(profile.clone()),
					benchmark,
					try_runtime,
					features: Some(features.join(","))
				},
				&mut cli,
			)?,
			project
		);
		cli.verify()
	}
}
