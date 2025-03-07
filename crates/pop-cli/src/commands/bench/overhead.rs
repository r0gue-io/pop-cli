use crate::{
	cli::{self, traits::Input},
	common::{
		bench::{ensure_runtime_binary_exists, guide_user_to_select_genesis_preset},
		prompt::display_message,
	},
};
use clap::{Args, Parser};
use frame_benchmarking_cli::OverheadCmd;
use pop_common::Profile;
use pop_parachains::generate_overhead_benchmarks;

#[derive(Args)]
pub struct BenchmarkOverhead {
	#[clap(flatten)]
	pub command: OverheadCmd,

	/// If this is set to true, no interactive prompts will be shown.
	#[clap(short = 'i', long)]
	pub skip_all: bool,
}

impl BenchmarkOverhead {
	pub async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking the execution overhead per-block and per-extrinsic")?;

		if !self.skip_all {
			if let Err(e) = self.interact(cli).await {
				return display_message(&e.to_string(), false, cli);
			};
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		if let Err(e) = self.run().await {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	async fn interact(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let cmd = &mut self.command;
		// If `chain` is provided, we don't prompt the user to configure the runtime.
		if cmd.shared_params.chain.is_none() {
			// No runtime path provided, auto-detect the runtime WASM binary. If not found,
			// build the runtime.
			if cmd.params.runtime.is_none() {
				cmd.params.runtime = Some(ensure_runtime_binary_exists(cli, &Profile::Release)?);
			}

			let runtime_policy = parse_genesis_builder_policy("runtime")?.params.genesis_builder;
			// No genesis builder, prompts user to select the genesis builder policy.
			if cmd.params.genesis_builder.is_none() {
				cmd.params.genesis_builder = runtime_policy;
			}

			// If the provided policy is `runtime`, we prompt the user to select the genesis
			// builder preset.
			if cmd.params.genesis_builder == runtime_policy {
				let runtime_path = cmd.params.runtime.as_ref().expect("No runtime found");
				guide_user_to_select_genesis_preset(
					cli,
					runtime_path,
					&cmd.params.genesis_builder_preset,
				)?;
			}
		}

		// Prompt user to update output path of the benchmarking results.
		if cmd.params.weight.weight_path.is_none() {
			let input = cli
				.input("Provide the output directory path for weight files")
				.required(true)
				.placeholder(".")
				.default_input(".")
				.interact()?;
			cmd.params.weight.weight_path =
				if !input.is_empty() { Some(input.into()) } else { None };
		}
		Ok(())
	}

	async fn run(&self) -> anyhow::Result<()> {
		generate_overhead_benchmarks(OverheadCmd {
			import_params: self.command.import_params.clone(),
			params: self.command.params.clone(),
			shared_params: self.command.shared_params.clone(),
		})
		.await
	}
}

fn parse_genesis_builder_policy(policy: &str) -> anyhow::Result<OverheadCmd> {
	OverheadCmd::try_parse_from([
		"",
		"--runtime",
		"dummy-runtime", // For parsing purpose.
		"--genesis-builder",
		policy,
	])
	.map_err(|e| {
		anyhow::anyhow!(format!(r#"Invalid genesis builder option {policy}: {}"#, e.to_string()))
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::MockCli, common::bench::get_mock_runtime};
	use pop_parachains::get_preset_names;
	use std::{env::current_dir, path::PathBuf};
	use tempfile::tempdir;

	#[test]
	fn parse_genesis_builder_policy_works() -> anyhow::Result<()> {
		for policy in ["runtime", "spec-runtime", "spec-genesis"] {
			parse_genesis_builder_policy(policy)?;
		}
		Ok(())
	}

	#[tokio::test]
	async fn benchmark_overhead_works() -> anyhow::Result<()> {
		let cwd = current_dir().unwrap_or(PathBuf::from("./"));
		let temp_dir = tempdir()?;
		let output_path = temp_dir.path().to_str().unwrap();
		let runtime_path = get_mock_runtime(true);
		let preset_names = get_preset_names(&runtime_path)
			.unwrap()
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();

		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				cwd.display()
			))
			.expect_input(
				"Please provide the path to the runtime or parachain project.",
				runtime_path.to_str().unwrap().to_string(),
			)
			.expect_select(
				"Select the genesis builder preset:",
				Some(true),
				true,
				Some(preset_names),
				0,
				None,
			)
			.expect_input(
				"Provide the output directory path for weight files",
				output_path.to_string(),
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking and generating weight file...")
			.expect_outro("Benchmark completed successfully!");

		let cmd = OverheadCmd::try_parse_from(["", "--warmup=1", "--repeat=1"])?;
		BenchmarkOverhead { command: cmd, skip_all: false }.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_overhead_non_interactive_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let output_path = temp_dir.path().to_str().unwrap();
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking and generating weight file...")
			.expect_outro("Benchmark completed successfully!");
		let cmd = OverheadCmd::try_parse_from([
			"",
			"--runtime",
			get_mock_runtime(false).to_str().unwrap(),
			"--warmup=1",
			"--repeat=1",
			"--weight-path",
			output_path,
		])?;
		BenchmarkOverhead { command: cmd, skip_all: true }.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_overhead_without_runtime_benchmarks_feature_fails() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_warning("NOTE: this may take some time...")
			.expect_outro_cancel(
				"Failed to run benchmarking: Invalid input: Need directory as --weight-path",
			);
		let cmd = OverheadCmd::try_parse_from([
			"",
			"--runtime",
			get_mock_runtime(false).to_str().unwrap(),
			"--weight-path=weights.rs",
		])?;
		BenchmarkOverhead { command: cmd, skip_all: true }.execute(&mut cli).await?;
		cli.verify()
	}
}
