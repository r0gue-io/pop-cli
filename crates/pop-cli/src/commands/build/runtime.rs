// SPDX-License-Identifier: GPL-3.0

pub use crate::common::runtime::Feature::{self, *};
use crate::{
	cli::{self},
	common::{prompt::display_message, runtime::build_runtime},
};
use pop_common::Profile;
use std::{collections::HashSet, path::PathBuf};

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
	/// The image tag to be used when we build a runtime deterministically
	pub(crate) tag: Option<String>,
	/// List of features the project is built with.
	pub(crate) features: Vec<String>,
}

impl BuildRuntime {
	/// Executes the build process.
	pub(crate) async fn execute(
		self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<serde_json::Value> {
		let is_parachain = pop_chains::is_supported(&self.path);
		let is_runtime = pop_chains::runtime::is_supported(&self.path);
		// `pop build runtime` must be run inside a parachain project or a specific runtime folder.
		if !is_parachain && !is_runtime {
			return display_message(
				"🚫 Can't build a runtime. Must be at the root of the chain project or a runtime.",
				false,
				cli,
			);
		}
		let (binary_path, runtime_path) = self.build(cli).await?;
		Ok(serde_json::json!({
			"binary_path": binary_path,
			"runtime_path": runtime_path,
		}))
	}

	async fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<(PathBuf, PathBuf)> {
		// Enable the features based on the user's input.
		let mut features = HashSet::new();
		self.features.iter().for_each(|f| {
			features.insert(f.as_str().into());
		});
		if self.benchmark {
			features.insert(Benchmark);
		}
		if self.try_runtime {
			features.insert(TryRuntime);
		}

		let mut features: Vec<_> = features.into_iter().collect();
		features.sort();
		cli.intro(if features.is_empty() {
			"Building your runtime".to_string()
		} else {
			let joined = features.iter().map(|feat| feat.as_ref()).collect::<Vec<_>>().join(",");
			format!("Building your runtime with features: {joined}")
		})?;

		if self.profile == Profile::Debug {
			cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
		}
		let workspace_root = rustilities::manifest::find_workspace_manifest(&self.path);
		let target_path = self
			.profile
			.target_directory(&workspace_root.unwrap_or(self.path.to_path_buf()))
			.join("wbuild");
		build_runtime(
			cli,
			&self.path,
			&target_path,
			&self.profile,
			&features,
			self.deterministic,
			self.tag,
		)
		.await
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
	use std::{fs, path::Path};
	use strum::VariantArray;

	#[tokio::test]
	async fn build_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let runtime_name = "mock_runtime";
		cmd("cargo", ["new", "--lib", runtime_name]).dir(path).run()?;

		// Create a runtime directory
		let target_dir = path.join(runtime_name);
		add_feature(target_dir.as_path(), ("try-runtime".to_string(), vec![]))?;
		add_feature(target_dir.as_path(), ("runtime-benchmarks".to_string(), vec![]))?;
		add_feature(target_dir.as_path(), ("dummy-feature".to_string(), vec![]))?;

		let project_path = path.join(runtime_name);
		let features = &[Benchmark, TryRuntime, Other("dummy-feature".to_string())];
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
			test_build(&project_path, &binary_path, profile, &[]).await?;

			// Build with one feature.
			test_build(&project_path, &binary_path, profile, &[Benchmark]).await?;

			// Build with multiple features.
			test_build(&project_path, &binary_path, profile, features).await?;
		}
		Ok(())
	}

	async fn test_build(
		project_path: &Path,
		binary_path: &Path,
		profile: &Profile,
		features: &[Feature],
	) -> anyhow::Result<()> {
		let mut raw_features: Vec<String> =
			features.iter().map(|feat| feat.as_ref().to_string()).collect();
		raw_features.sort();
		let mut cli = MockCli::new().expect_intro(if features.is_empty() {
			"Building your runtime".to_string()
		} else {
			format!("Building your runtime with features: {}", raw_features.join(","))
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
			profile: *profile,
			benchmark: features.contains(&Benchmark),
			try_runtime: features.contains(&TryRuntime),
			deterministic: false,
			features: raw_features,
			tag: None,
		}
		.build(&mut cli)
		.await?;
		cli.verify()
	}
}
