use crate::{
	cli::{self},
	common::bench::{ensure_runtime_binary_exists, guide_user_to_select_genesis_preset},
};
use clap::{Args, Parser};
use frame_benchmarking_cli::OverheadCmd;
use pop_common::Profile;
use pop_parachains::generate_overhead_benchmarks;

use super::display_message;

#[derive(Args)]
pub struct BenchmarkOverhead {
	#[clap(flatten)]
	pub command: OverheadCmd,
}

impl BenchmarkOverhead {
	pub async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let cmd = &mut self.command;

		cli.intro("Benchmarking the execution overhead per-block and per-extrinsic")?;

		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build
		// the runtime.
		if cmd.params.runtime.is_none() {
			match ensure_runtime_binary_exists(cli, &Profile::Release) {
				Ok(runtime_binary_path) => cmd.params.runtime = Some(runtime_binary_path),
				Err(e) => {
					return display_message(&e.to_string(), false, cli);
				},
			}
		}
		// No genesis builder, prompts user to select the genesis builder policy.
		if cmd.params.genesis_builder.is_none() {
			cmd.params.genesis_builder =
				parse_genesis_builder_policy("runtime")?.params.genesis_builder;
			let preset = cmd.params.genesis_builder_preset.clone();
			let runtime_path = cmd.params.runtime.as_ref().expect("No runtime found");
			if let Err(e) = guide_user_to_select_genesis_preset(cli, &runtime_path, &preset) {
				return display_message(&e.to_string(), false, cli);
			};
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		if let Err(e) = generate_overhead_benchmarks(OverheadCmd {
			import_params: cmd.import_params.clone(),
			params: cmd.params.clone(),
			shared_params: cmd.shared_params.clone(),
		})
		.await
		{
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
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
	use tempfile::tempdir;

	#[test]
	fn parse_genesis_builder_policy_works() -> anyhow::Result<()> {
		for policy in ["runtime", "spec-runtime", "spec-genesis"] {
			parse_genesis_builder_policy(policy)?;
		}
		Ok(())
	}

	#[tokio::test]
	async fn benchmarking_overhead_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime(true);
		let preset_names = get_preset_names(&runtime_path)
			.unwrap()
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_warning("NOTE: this may take some time...")
			.expect_select(
				"Select the genesis builder preset:",
				Some(true),
				true,
				Some(preset_names),
				0,
			)
			.expect_info("Benchmarking and generating weight file...")
			.expect_outro("Benchmark completed successfully!");

		let cmd = OverheadCmd::try_parse_from([
			"",
			"--runtime",
			get_mock_runtime(true).to_str().unwrap(),
			"--warmup=1",
			"--repeat=1",
			"--weight-path",
			temp_dir.path().to_str().unwrap(),
		])?;

		BenchmarkOverhead { command: cmd }.execute(&mut cli).await?;
		Ok(())
	}
}
