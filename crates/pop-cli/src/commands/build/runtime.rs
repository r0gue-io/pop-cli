use crate::{
	cli,
	common::runtime::{
		build_deterministic_runtime, build_runtime,
		Feature::{self, Benchmark, TryRuntime},
	},
	style::style,
};
use cliclack::spinner;
use pop_common::{manifest::from_path, Profile};
use std::{
	env::current_dir,
	path::{Path, PathBuf},
};

// Configuration for building a runtime.
pub struct BuildRuntime {
	/// Directory path for your runtime project.
	pub(crate) path: PathBuf,
	/// Build profile.
	pub(crate) profile: Profile,
	/// Whether to build the runtime with `runtime-benchmarks` feature.
	pub(crate) benchmark: bool,
	/// Whether to build the runtime with `try-runtime` feature.
	pub(crate) try_runtime: bool,
	/// Whether to build a runtime deterministically.
	pub(crate) deterministic: bool,
}

impl BuildRuntime {
	/// Executes the build process.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		let root = current_dir().unwrap_or(PathBuf::from("./"));
		let target_path = self.profile.target_directory(root.as_path());
		self.build(&mut cli::Cli, &target_path)
	}

	fn build(self, cli: &mut impl cli::traits::Cli, project_root: &Path) -> anyhow::Result<()> {
		let spinner = spinner();
		let manifest = from_path(Some(self.path.as_path()))?;
		let package = manifest.package();
		let name = package.clone().name;

		// Enable the features based on the user's input.
		let mut features = vec![];
		if self.benchmark {
			features.push(Benchmark);
		}
		if self.try_runtime {
			features.push(TryRuntime);
		}

		cli.intro(if features.is_empty() {
			format!("Building {:?} runtime", name)
		} else {
			let joined = features
				.iter()
				.map(|feat| feat.as_ref().to_string())
				.collect::<Vec<String>>()
				.join(",");
			format!("Building {:?} runtime with features: {}", name, joined)
		})?;
		if self.deterministic {
			spinner.start("Building runtime deterministically...");
			build_deterministic_runtime(&mut cli::Cli, &spinner, name, self.profile, self.path)?;
			spinner.stop("");
		} else {
			self.build_non_determinisic(&mut cli::Cli, project_root, features)?;
		}
		Ok(())
	}

	fn build_non_determinisic(
		self,
		cli: &mut impl cli::traits::Cli,
		project_root: &Path,
		features: Vec<Feature>,
	) -> anyhow::Result<()> {
		if self.profile == Profile::Debug {
			cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
		}
		// Build runtime.
		let target_path = self.profile.target_directory(project_root).join("wbuild");
		let binary = build_runtime(cli, &self.path, &target_path, &self.profile, features)?;
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
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;
	use duct::cmd;
	use pop_common::manifest::{add_feature, add_production_profile};
	use std::fs;
	use strum::VariantArray;

	#[test]
	fn build_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let runtime_name = "mock_runtime";
		cmd("cargo", ["new", "--lib", runtime_name]).dir(&path).run()?;

		// Create a runtime directory
		let target_dir = path.join(runtime_name);
		add_feature(target_dir.as_path(), ("try-runtime".to_string(), vec![]))?;
		add_feature(target_dir.as_path(), ("runtime-benchmarks".to_string(), vec![]))?;

		let project_path = path.join(runtime_name);
		let features = &[Benchmark.as_ref(), TryRuntime.as_ref()];
		add_production_profile(&project_path)?;
		for feature in features {
			add_feature(&project_path, (feature.to_string(), vec![]))?;
		}

		for profile in Profile::VARIANTS {
			let target_path = profile
				.target_directory(&target_dir)
				.join(format!("./wbuild/{}/{}.wasm", runtime_name, runtime_name));
			fs::create_dir_all(target_path)?;

			// Build without features.
			test_build(&project_path, profile, &[], false)?;

			// Build with one feature.
			test_build(&project_path, profile, &[Benchmark.as_ref()], false)?;

			// Build with multiple features.
			test_build(&project_path, profile, features, false)?;
		}
		Ok(())
	}

	fn test_build(
		project_path: &PathBuf,
		profile: &Profile,
		features: &[&str],
		deterministic: bool,
	) -> anyhow::Result<()> {
		let manifest = from_path(Some(project_path.as_path()))?;
		let package = manifest.package();
		let name = package.clone().name;

		let mut cli = MockCli::new().expect_intro(if features.is_empty() {
			format!("Building {:?} runtime", name)
		} else {
			format!("Building {:?} runtime with features: {}", name, features.join(","))
		});

		if deterministic {
		} else {
			if profile == &Profile::Debug {
				cli = cli.expect_warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...");
			}
			cli = cli
				.expect_warning("NOTE: this may take some time...")
				.expect_info(format!("The rutnime was built in {profile} mode."))
				.expect_outro("\n✅ Runtime built successfully.\n");
		}

		BuildRuntime {
			path: project_path.clone(),
			profile: profile.clone(),
			benchmark: features.contains(&Benchmark.as_ref()),
			try_runtime: features.contains(&TryRuntime.as_ref()),
			deterministic: false,
		}
		.build(&mut cli, project_path)?;
		cli.verify()
	}
}
