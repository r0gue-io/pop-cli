// SPDX-License-Identifier: GPL-3.0

pub use crate::common::runtime::Feature::{self, *};
use crate::{
	cli::{self},
	common::{
		prompt::display_message,
		runtime::{build_runtime, ensure_runtime_binary_exists},
	},
};
use pop_common::{find_workspace_toml, Profile};
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
		let cli = &mut cli::Cli;
		let current_dir = current_dir().unwrap_or(PathBuf::from("./"));
		let is_parachain = pop_parachains::is_supported(Some(&current_dir))?;
		let is_runtime = pop_parachains::runtime::is_supported(Some(&current_dir))?;
		// `pop build runtime` must be run inside a parachain project or a specific runtime folder.
		if !is_parachain && !is_runtime {
			return display_message(
				"🚫 Can't build a runtime. Must be at the root of the chain project or a runtime.",
				false,
				cli,
			)
		}
		self.build(cli, &current_dir, !is_runtime && is_parachain)
	}

	fn build(
		self,
		cli: &mut impl cli::traits::Cli,
		path: &Path,
		is_parachain: bool,
	) -> anyhow::Result<()> {
		// Enable the features based on the user's input.
		let mut features = vec![];
		if self.benchmark {
			features.push(Benchmark);
		}
		if self.try_runtime {
			features.push(TryRuntime);
		}

		cli.intro(if features.is_empty() {
			"Building your runtime".to_string()
		} else {
			let joined = features
				.iter()
				.map(|feat| feat.as_ref().to_string())
				.collect::<Vec<String>>()
				.join(",");
			format!("Building your runtime with features: {}", joined)
		})?;

		if is_parachain {
			ensure_runtime_binary_exists(
				cli,
				path,
				&self.profile,
				&features,
				true,
				self.deterministic,
			)?;
		}
		if self.profile == Profile::Debug {
			cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
		}
		let workspace_root = find_workspace_toml(path);
		let target_path = self
			.profile
			.target_directory(&workspace_root.unwrap_or(path.to_path_buf()))
			.join("wbuild");
		build_runtime(cli, &self.path, &target_path, &self.profile, &features, self.deterministic)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::common::runtime::Feature;
	use cli::MockCli;
	use console::style;
	use duct::cmd;
	use pop_common::manifest::{add_feature, add_production_profile};
	use std::fs;
	use strum::VariantArray;

	#[test]
	fn build_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let runtime_name = "mock_runtime";
		cmd("cargo", ["new", "--lib", runtime_name]).dir(&path).run()?;

		// Create a runtime directory
		let target_dir = path.join(runtime_name);
		add_feature(target_dir.as_path(), ("try-runtime".to_string(), vec![]))?;
		add_feature(target_dir.as_path(), ("runtime-benchmarks".to_string(), vec![]))?;

		let project_path = path.join(runtime_name);
		let features = &[Benchmark, TryRuntime];
		add_production_profile(&project_path)?;
		for feature in features {
			add_feature(&project_path, (feature.as_ref().to_string(), vec![]))?;
		}

		for profile in Profile::VARIANTS {
			let binary_path = profile
				.target_directory(&target_dir)
				.join(format!("wbuild/{}/{}.wasm", runtime_name, runtime_name));
			fs::create_dir_all(&binary_path)?;

			// Build without features.
			test_build(&project_path, &binary_path, profile, &[])?;

			// Build with one feature.
			test_build(&project_path, &binary_path, profile, &[Benchmark])?;

			// Build with multiple features.
			test_build(&project_path, &binary_path, profile, features)?;
		}
		Ok(())
	}

	fn test_build(
		project_path: &Path,
		binary_path: &Path,
		profile: &Profile,
		features: &[Feature],
	) -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_intro(if features.is_empty() {
			"Building your runtime".to_string()
		} else {
			let features: Vec<String> =
				features.iter().map(|feat| feat.as_ref().to_string()).collect();
			format!("Building your runtime with features: {}", features.join(","))
		});
		if profile == &Profile::Debug {
			cli = cli.expect_warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...");
		}
		let generated_files = [format!("Binary generated at: {}", binary_path.display())];
		let generated_files: Vec<_> = generated_files
			.iter()
			.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
			.collect();
		cli = cli
			.expect_warning("NOTE: this may take some time...")
			.expect_info(format!("Building your runtime in {profile} mode..."))
			.expect_info(format!("The runtime was built in {profile} mode."))
			.expect_success("\n✅ Runtime built successfully.\n")
			.expect_success(format!("Generated files:\n{}", generated_files.join("\n")))
			.expect_outro(format!(
				"Need help? Learn more at {}\n",
				style("https://learn.onpop.io").magenta().underlined()
			));

		BuildRuntime {
			path: project_path.to_path_buf().clone(),
			profile: profile.clone(),
			benchmark: features.contains(&Benchmark),
			try_runtime: features.contains(&TryRuntime),
			deterministic: false,
		}
		.build(&mut cli, project_path, false)?;
		cli.verify()
	}
}
