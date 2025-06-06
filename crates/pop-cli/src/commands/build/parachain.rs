// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	common::runtime::Feature::{Benchmark, TryRuntime},
	style::style,
};
use pop_common::Profile;
use pop_parachains::build_parachain;
use std::path::PathBuf;

use super::{PACKAGE, PARACHAIN};

// Configuration for building a parachain.
pub struct BuildParachain {
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
}

impl BuildParachain {
	/// Executes the build process.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	/// Builds a parachain.
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		let project = if self.package.is_some() { PACKAGE } else { PARACHAIN };

		// Enable the features based on the user's input.
		let mut features = vec![];
		if self.benchmark {
			features.push(Benchmark.as_ref());
		}
		if self.try_runtime {
			features.push(TryRuntime.as_ref());
		}
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
		let binary = build_parachain(&self.path, self.package, &self.profile, None, features)?;
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

		Ok(project)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;
	use duct::cmd;
	use pop_common::manifest::{add_feature, add_production_profile};
	use std::{fs, io::Write, path::Path};
	use strum::VariantArray;

	// Function that generates a Cargo.toml inside node directory for testing.
	fn generate_mock_node(temp_dir: &Path) -> anyhow::Result<()> {
		// Create a node directory
		let target_dir = temp_dir.join("node");
		fs::create_dir(&target_dir)?;
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
		Ok(())
	}

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		let features = &[Benchmark.as_ref(), TryRuntime.as_ref()];
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		add_production_profile(&project_path)?;
		for feature in features {
			add_feature(&project_path, (feature.to_string(), vec![]))?;
		}
		generate_mock_node(&project_path)?;

		for package in [None, Some(name.to_string())] {
			for profile in Profile::VARIANTS {
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
		project_path: &PathBuf,
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
			BuildParachain {
				path: project_path.clone(),
				package: package.clone(),
				profile: profile.clone(),
				benchmark: features.contains(&Benchmark.as_ref()),
				try_runtime: features.contains(&TryRuntime.as_ref()),
			}
			.build(&mut cli)?,
			project
		);
		cli.verify()
	}
}
