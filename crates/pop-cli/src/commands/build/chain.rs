// SPDX-License-Identifier: GPL-3.0

use super::{PACKAGE, PARACHAIN};
use crate::{
	cli,
	common::{
		builds::{ChainPath, create_chain_spec_builder},
		runtime::Feature::{Benchmark, TryRuntime},
	},
	style::style,
};
use pop_common::Profile;
use std::{collections::HashSet, path::PathBuf};

/// Configuration for building a parachain.
pub struct BuildChain {
	/// Directory path for your project.
	pub(crate) path: PathBuf,
	/// The package to be built.
	pub(crate) package: Option<String>,
	/// Build profile.
	pub(crate) profile: Profile,
	/// Whether to build the parachain with `runtime-benchmarks` feature.
	pub(crate) benchmark: bool,
	/// Whether to build the parachain with `try-runtime` feature.
	pub(crate) try_runtime: bool,
	/// List of features to build the node or runtime with.
	pub(crate) features: Vec<String>,
}

impl BuildChain {
	/// Executes the build process.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	/// Executes the build process in JSON mode and returns the built artifact path.
	pub(crate) fn execute_json(self) -> anyhow::Result<PathBuf> {
		let mut json_cli = crate::cli::JsonCli;
		Ok(self.build_inner(&mut json_cli, true)?.1)
	}

	/// Builds a chain.
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		Ok(self.build_inner(cli, false)?.0)
	}

	fn build_inner(
		self,
		cli: &mut impl cli::traits::Cli,
		redirect_output_to_stderr: bool,
	) -> anyhow::Result<(&'static str, PathBuf)> {
		let project = if self.package.is_some() { PACKAGE } else { PARACHAIN };

		// Enable the features based on the user's input.
		let mut features = HashSet::new();
		self.features.iter().for_each(|f| {
			features.insert(f.clone());
		});
		if self.benchmark {
			features.insert(Benchmark.as_ref().to_string());
		}
		if self.try_runtime {
			features.insert(TryRuntime.as_ref().to_string());
		}
		let mut features: Vec<_> = features.iter().map(|f| f.as_str()).collect();
		features.sort();
		cli.intro(if features.is_empty() {
			format!("Building your {project}")
		} else {
			format!("Building your {project} with features: {}", features.join(","))
		})?;
		if self.profile == Profile::Debug {
			cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
		}

		// Build parachain.
		cli.warning("NOTE: this may take some time...")?;
		let builder = create_chain_spec_builder(
			ChainPath::Base(self.path.to_path_buf()),
			&self.profile,
			false,
			cli,
		)?;
		let features_arr: Vec<_> = features.into_iter().map(|s| s.to_string()).collect();
		let binary = builder.build(features_arr.as_slice(), redirect_output_to_stderr)?;
		cli.info(format!("The {project} was built in {} mode.", self.profile))?;
		cli.outro("Build completed successfully!")?;
		let generated_files = [format!("Binary generated at: {}", binary.display())];
		let generated_files: Vec<_> = generated_files
			.iter()
			.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
			.collect();
		cli.success(format!("Generated files:\n{}", generated_files.join("\n")))?;
		cli.outro(format!(
			"Need help? Learn more at {}\n",
			style("https://learn.onpop.io").magenta().underlined()
		))?;

		Ok((project, binary))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;

	use pop_common::manifest::{add_feature, add_production_profile};
	use std::{fs, io::Write, path::Path};
	use strum::VariantArray;

	// Function that generates a Cargo.toml inside node directory for testing.
	fn generate_mock_node(temp_dir: &Path) -> anyhow::Result<PathBuf> {
		// Create a node directory
		let target_dir = temp_dir.join("node");
		fs::create_dir(&target_dir)?;
		fs::create_dir(target_dir.join("src"))?;
		fs::write(target_dir.join("src").join("main.rs"), "fn main() {}")?;
		// Create a Cargo.toml file
		let mut toml_file = fs::File::create(target_dir.join("Cargo.toml"))?;
		writeln!(
			toml_file,
			r#"
			[package]
			name = "hello_world"
			version = "0.1.0"

			[dependencies]
			"#
		)?;
		Ok(target_dir)
	}

	// Function that generates a Cargo.toml inside runtime directory for testing.
	fn generate_mock_runtime(temp_dir: &Path) -> anyhow::Result<PathBuf> {
		// Create a runtime directory
		let target_dir = temp_dir.join("runtime");
		fs::create_dir(&target_dir)?;
		fs::create_dir(target_dir.join("src"))?;
		fs::write(target_dir.join("src").join("lib.rs"), "")?;
		// Create a Cargo.toml file
		let mut toml_file = fs::File::create(target_dir.join("Cargo.toml"))?;
		writeln!(
			toml_file,
			r#"
			[package]
			name = "hello_world_runtime"
			version = "0.1.0"

			[dependencies]
			"#
		)?;
		Ok(target_dir)
	}

	#[test]
	fn build_with_node_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let name = "hello_world";
		let project_path = path.join(name);
		fs::create_dir(&project_path)?;
		let workspace_toml = project_path.join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["node"]

[workspace.package]
name = "test-workspace"
"#,
		)?;
		add_production_profile(&project_path)?;
		let node_project_path = generate_mock_node(&project_path)?;
		let features = &[Benchmark.as_ref(), TryRuntime.as_ref()];
		for feature in features {
			add_feature(&node_project_path, (feature.to_string(), vec![]))?;
		}

		for package in [None, Some(name.to_string())] {
			// Use representative profiles (Debug + Release) to avoid redundant builds.
			// Production inherits from Release, so it only differs in LTO/codegen-units.
			for profile in [Profile::Debug, Profile::Release] {
				// Build without features.
				test_build(package.clone(), &project_path, &profile, &[])?;

				// Build with one feature.
				test_build(package.clone(), &project_path, &profile, &[Benchmark.as_ref()])?;

				// Build with multiple features.
				test_build(package.clone(), &project_path, &profile, features)?;
			}
		}
		Ok(())
	}

	#[test]
	fn build_with_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let name = "hello_world";
		let project_path = path.join(name);
		fs::create_dir(&project_path)?;
		let workspace_toml = project_path.join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime"]

[workspace.package]
name = "test-workspace"
"#,
		)?;
		add_production_profile(&project_path)?;
		let runtime_project_path = generate_mock_runtime(&project_path)?;
		let features = &[Benchmark.as_ref(), TryRuntime.as_ref()];
		for feature in features {
			add_feature(&runtime_project_path, (feature.to_string(), vec![]))?;
		}

		for package in [None, Some(name.to_string())] {
			for profile in Profile::VARIANTS {
				// Mock runtime wasm build
				let wasm_build_dir = project_path
					.join("target")
					.join(profile.to_string())
					.join("wbuild")
					.join("hello_world_runtime");
				fs::create_dir_all(&wasm_build_dir)?;
				fs::write(wasm_build_dir.join("hello_world_runtime.wasm"), "")?;

				// Build without features.
				test_build(package.clone(), &project_path, profile, &[])?;

				// Build with one feature.
				test_build(package.clone(), &project_path, profile, &[Benchmark.as_ref()])?;

				// Build with multiple features.
				test_build(package.clone(), &project_path, profile, features)?;
			}
		}
		Ok(())
	}

	fn test_build(
		package: Option<String>,
		project_path: &Path,
		profile: &Profile,
		features: &[&str],
	) -> anyhow::Result<()> {
		let project = if package.is_some() { PACKAGE } else { PARACHAIN };
		let mut cli = MockCli::new()
			.expect_intro(if features.is_empty() {
				format!("Building your {project}")
			} else {
				format!("Building your {project} with features: {}", features.join(","))
			})
			.expect_warning("NOTE: this may take some time...")
			.expect_info(format!("The {project} was built in {profile} mode."))
			.expect_outro("Build completed successfully!");

		if profile == &Profile::Debug {
			cli = cli.expect_warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...");
		}

		assert_eq!(
			BuildChain {
				path: project_path.to_path_buf(),
				package: package.clone(),
				profile: *profile,
				benchmark: features.contains(&Benchmark.as_ref()),
				try_runtime: features.contains(&TryRuntime.as_ref()),
				features: features.iter().map(|f| f.to_string()).collect(),
			}
			.build(&mut cli)?,
			project
		);
		cli.verify()
	}
}
