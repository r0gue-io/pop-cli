use std::{env::current_dir, path::PathBuf};

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
pub(crate) struct BenchmarkOverhead {
	/// Commmand to benchmark the execution overhead per-block and per-extrinsic.
	#[clap(flatten)]
	pub command: OverheadCmd,
}

impl BenchmarkOverhead {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking the execution overhead per-block and per-extrinsic")?;

		if let Err(e) = self.interact(cli).await {
			return display_message(&e.to_string(), false, cli);
		};

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		let result = self.run().await;

		// Display the benchmarking command.
		cliclack::log::remark("\n")?;
		cli.success(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	async fn interact(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let cmd = &mut self.command;
		// If `chain` is provided, we don't prompt the user to configure the runtime.
		if cmd.shared_params.chain.is_none() {
			// No runtime path provided, auto-detect the runtime binary. If not found,
			// build the runtime.
			if cmd.params.runtime.is_none() {
				cmd.params.runtime = Some(ensure_runtime_binary_exists(
					cli,
					&current_dir().unwrap_or(PathBuf::from("./")),
					&Profile::Release,
				)?);
			}

			let runtime_policy = parse_genesis_builder_policy("runtime")?.params.genesis_builder;
			// No genesis builder, hard-coded the policy with `runtime`.
			if cmd.params.genesis_builder.is_none() {
				cmd.params.genesis_builder = runtime_policy;
			}

			// If the provided policy is `runtime`, we prompt the user to select the genesis
			// builder preset.
			if cmd.params.genesis_builder == runtime_policy {
				let runtime_path = cmd
					.params
					.runtime
					.as_ref()
					.ok_or_else(|| anyhow::anyhow!("No runtime found"))?;
				cmd.params.genesis_builder_preset = guide_user_to_select_genesis_preset(
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

	fn display(&self) -> String {
		let mut args = vec!["pop bench overhead".to_string()];
		let mut arguments: Vec<String> = std::env::args().skip(3).collect();

		// Check if the arguments are provided by the user.
		let mut print_runtime = true;
		let mut print_genesis_builder = true;
		let mut print_genesis_builder_preset = true;
		let mut print_weight_path = true;
		for argument in arguments.iter() {
			print_runtime = print_runtime && !argument.starts_with("--runtime");
			print_genesis_builder =
				print_genesis_builder && !argument.starts_with("--genesis-builder");
			print_genesis_builder_preset =
				print_genesis_builder_preset && !argument.starts_with("--genesis-builder-preset");
			print_weight_path = print_weight_path && !argument.starts_with("--weight-path");
		}

		if print_runtime {
			if let Some(ref runtime) = self.command.params.runtime {
				arguments.push(format!("--runtime={}", runtime.display()));
			}
		}
		if print_genesis_builder {
			arguments.push("--genesis-builder=runtime".to_string());
		}
		if print_genesis_builder_preset {
			arguments.push(format!(
				"--genesis-builder-preset={}",
				self.command.params.genesis_builder_preset
			));
		}
		if print_weight_path {
			if let Some(ref weight_path) = self.command.params.weight.weight_path {
				arguments.push(format!("--weight-path={}", weight_path.display()));
			}
		}
		args.extend(arguments);
		args.join(" ")
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
	fn parse_genesis_builder_policy_works() {
		for policy in ["runtime", "spec-runtime", "spec-genesis"] {
			assert!(parse_genesis_builder_policy(policy).is_ok());
		}
	}

	#[test]
	fn display_works() {
		assert_eq!(
			BenchmarkOverhead { command: OverheadCmd::try_parse_from([""]).unwrap() }.display(),
			"pop bench overhead --genesis-builder=runtime --genesis-builder-preset=development"
		);
		assert_eq!(
			BenchmarkOverhead {
				command: OverheadCmd::try_parse_from([
					"",
					"--runtime",
					"dummy-runtime",
					"--genesis-builder=runtime",
					"--weight-path=weights.rs",
				])
				.unwrap()
			}
			.display(),
			"pop bench overhead --runtime=dummy-runtime --genesis-builder=runtime \
			--genesis-builder-preset=development --weight-path=weights.rs"
		);
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
				"Please provide the path to the runtime.",
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
			// Unable to mock the `std::env::args` for testing. In production, in must include
			// `--warmup` and `--repeat`.
			.expect_success(format!(
				"pop bench overhead --runtime={} --genesis-builder=runtime \
				--genesis-builder-preset=development --weight-path={}",
				runtime_path.display(),
				output_path.to_string(),
			))
			.expect_outro("Benchmark completed successfully!");

		let cmd = OverheadCmd::try_parse_from(["", "--warmup=1", "--repeat=1"])?;
		BenchmarkOverhead { command: cmd }.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_overhead_fails_with_no_feature() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(false);
		let preset_names = get_preset_names(&runtime_path)
			.unwrap()
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_select(
				"Select the genesis builder preset:",
				Some(true),
				true,
				Some(preset_names),
				0,
				None,
			)
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
		BenchmarkOverhead { command: cmd }.execute(&mut cli).await?;
		cli.verify()
	}
}
