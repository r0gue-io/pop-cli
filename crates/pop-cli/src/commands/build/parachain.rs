// SPDX-License-Identifier: GPL-3.0

use crate::{cli, style::style};
use clap::Args;
use pop_parachains::{
	build_parachain, export_wasm_file, generate_genesis_state_file, generate_plain_chain_spec,
	generate_raw_chain_spec, Profile,
};
use std::path::PathBuf;
#[cfg(not(test))]
use std::{thread::sleep, time::Duration};

const PLAIN_CHAIN_SPEC_FILE_NAME: &str = "plain-parachain-chainspec.json";
const RAW_CHAIN_SPEC_FILE_NAME: &str = "raw-parachain-chainspec.json";

#[derive(Args)]
pub struct BuildParachainCommand {
	/// Directory path for your project [default: current directory].
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// The package to be built.
	#[arg(short = 'p', long)]
	pub(crate) package: Option<String>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short, long, default_value = "true")]
	pub(crate) release: bool,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long = "id")]
	pub(crate) id: Option<u32>,
	// Deprecation flag, used to specify whether the deprecation warning is shown.
	#[clap(skip)]
	pub(crate) valid: bool,
}

impl BuildParachainCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	/// Builds a parachain.
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		let project = if self.package.is_some() { "package" } else { "parachain" };
		cli.intro(format!("Building your {project}"))?;

		// Show warning if specified as deprecated.
		if !self.valid {
			cli.warning("NOTE: this command is deprecated. Please use `pop build` (or simply `pop b`) in future...")?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3))
		} else {
			if !self.release {
				cli.warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...")?;
				#[cfg(not(test))]
				sleep(Duration::from_secs(3))
			}
		}

		// Build parachain.
		cli.warning("NOTE: this may take some time...")?;
		let project_path = self.path.unwrap_or_else(|| PathBuf::from("./"));
		let mode: Profile = self.release.into();
		let binary = build_parachain(&project_path, self.package, &mode, None)?;
		cli.info(format!("The {project} was built in {:?} mode.", mode))?;
		cli.outro("Build completed successfully!")?;
		let mut generated_files = vec![format!("Binary generated at: {}", binary.display())];

		// If `para_id` is provided, generate the chain spec
		if let Some(para_id) = self.id {
			let plain_chain_spec = project_path.join(PLAIN_CHAIN_SPEC_FILE_NAME);
			generate_plain_chain_spec(&binary, &plain_chain_spec, para_id)?;
			generated_files.push(format!(
				"Plain text chain specification file generated at: {}",
				plain_chain_spec.display()
			));
			let raw_chain_spec =
				generate_raw_chain_spec(&binary, &plain_chain_spec, RAW_CHAIN_SPEC_FILE_NAME)?;
			generated_files.push(format!(
				"New raw chain specification file generated at: {}",
				raw_chain_spec.display()
			));
			let wasm_file_name = format!("para-{}-wasm", para_id);
			let wasm_file = export_wasm_file(&binary, &raw_chain_spec, &wasm_file_name)?;
			generated_files.push(format!(
				"WebAssembly runtime file exported at: {}",
				wasm_file.display().to_string()
			));
			let genesis_file_name = format!("para-{}-genesis-state", para_id);
			let genesis_state_file =
				generate_genesis_state_file(&binary, &raw_chain_spec, &genesis_file_name)?;
			generated_files.push(format!(
				"Genesis State exported at {} file",
				genesis_state_file.display().to_string()
			));
			console::Term::stderr().clear_last_lines(5)?;
		}
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
	use std::{fs, io::Write, path::Path};

	// Function that generates a Cargo.toml inside node folder for testing.
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
			authors.workspace = true
			edition.workspace = true
			homepage.workspace = true
			license.workspace = true
			repository.workspace = true

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
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		generate_mock_node(&temp_dir.path().join(name))?;

		for package in [None, Some(name.to_string())] {
			for release in [false, true] {
				for valid in [false, true] {
					let project = if package.is_some() { "package" } else { "parachain" };
					let mode = if release { "RELEASE" } else { "DEBUG" };
					let generated_files: Vec<_> = vec![format!(
						"Binary generated at: {}",
						&temp_dir.path().join(name).display()
					)]
					.iter()
					.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
					.collect();
					let mut cli = MockCli::new()
						.expect_intro(format!("Building your {project}"))
						.expect_warning("NOTE: this may take some time...")
						.expect_info(format!("The {project} was built in {mode} mode."))
						.expect_outro("Build completed successfully!")
						.expect_success(format!("Generated files:\n{}", generated_files.join("\n")))
						.expect_outro(format!(
							"Need help? Learn more at {}\n",
							style("https://learn.onpop.io").magenta().underlined()
						));

					if !valid {
						cli = cli.expect_warning("NOTE: this command is deprecated. Please use `pop build` (or simply `pop b`) in future...");
					} else {
						if !release {
							cli = cli.expect_warning("NOTE: this command now defaults to DEBUG builds. Please use `--release` (or simply `-r`) for a release build...");
						}
					}

					assert_eq!(
						BuildParachainCommand {
							path: Some(path.join(name)),
							package: package.clone(),
							release,
							id: None,
							valid,
						}
						.build(&mut cli)?,
						project
					);

					cli.verify()?;
				}
			}
		}

		Ok(())
	}
}
