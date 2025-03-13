use crate::{
	cli::{self},
	common::{
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		prompt::display_message,
	},
};
use clap::Args;
use frame_benchmarking_cli::StorageCmd;
use pop_common::Profile;
use pop_parachains::generate_binary_benchmarks;
use std::{
	env::current_dir,
	path::{Path, PathBuf},
};

#[derive(Args)]
pub(crate) struct BenchmarkStorage {
	/// Command to benchmark the storage speed of a chain snapshot.
	#[clap(flatten)]
	pub command: StorageCmd,
	/// Build profile.
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
}

impl BenchmarkStorage {
	pub(crate) fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.benchmark(cli, &current_dir().unwrap_or(PathBuf::from("./")))
	}

	fn benchmark(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		target_path: &Path,
	) -> anyhow::Result<()> {
		cli.intro("Benchmarking the storage speed of a chain snapshot")?;

		if self.profile.is_none() {
			self.profile = Some(guide_user_to_select_profile(cli)?);
		};
		let binary_path = ensure_node_binary_exists(
			cli,
			target_path,
			self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
			vec!["runtime-benchmarks"],
		)?;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		let result = generate_binary_benchmarks(&binary_path, "storage");

		// Display the benchmarking command.
		cliclack::log::remark("\n")?;
		cli.info(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut args = vec!["pop bench storage".to_string()];
		let mut arguments: Vec<String> = std::env::args().skip(3).collect();
		if let Some(ref profile) = self.profile {
			arguments.push(format!("--profile={}", profile));
		}
		args.extend(arguments);
		args.join(" ")
	}
}

#[cfg(test)]
mod tests {
	use crate::cli::MockCli;
	use clap::Parser;
	use duct::cmd;
	use frame_benchmarking_cli::StorageCmd;
	use pop_common::Profile;
	use std::fs::{self, File};
	use strum::{EnumMessage, VariantArray};
	use tempfile::tempdir;

	use super::BenchmarkStorage;

	#[test]
	fn benchmark_storage_works() -> anyhow::Result<()> {
		let name = "node";
		let temp_dir = tempdir()?;
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let target_path = Profile::Debug.target_directory(temp_dir.path());

		fs::create_dir(&temp_dir.path().join("target"))?;
		fs::create_dir(&target_path)?;
		File::create(target_path.join("node"))?;

		// With `profile` provided.
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the storage speed of a chain snapshot")
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking and generating weight file...")
			.expect_info("pop bench storage --profile=debug")
			.expect_outro_cancel(
				// As we only mock the node to test the interactive flow, the returned error is
				// expected.
				"Failed to run benchmarking: Permission denied (os error 13)",
			);
		BenchmarkStorage {
			command: StorageCmd::try_parse_from(vec!["", "--state-version=1"])?,
			profile: Some(Profile::Debug),
		}
		.benchmark(&mut cli, temp_dir.path())?;
		cli.verify()?;

		// Prompt user to select `profile` if not provided.
		let profiles = Profile::VARIANTS
			.iter()
			.map(|profile| {
				(
					profile.get_message().unwrap_or(profile.as_ref()).to_string(),
					profile.get_detailed_message().unwrap_or_default().to_string(),
				)
			})
			.collect();
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the storage speed of a chain snapshot")
			.expect_select(
				"Choose the build profile of the binary that should be used: ",
				Some(true),
				true,
				Some(profiles),
				0,
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking and generating weight file...")
			.expect_info("pop bench storage --profile=debug")
			.expect_outro_cancel(
				// As we only mock the node to test the interactive flow, the returned error is
				// expected.
				"Failed to run benchmarking: Permission denied (os error 13)",
			);
		BenchmarkStorage {
			command: StorageCmd::try_parse_from(vec!["", "--state-version=1"])?,
			profile: None,
		}
		.benchmark(&mut cli, temp_dir.path())?;
		cli.verify()
	}
}
