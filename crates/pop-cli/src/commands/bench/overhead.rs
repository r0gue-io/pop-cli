use std::{env::current_dir, path::PathBuf};

use crate::{
	cli::{self, traits::Input},
	common::{
		bench::{
			check_omni_bencher_and_prompt, ensure_runtime_binary_exists,
			guide_user_to_select_genesis_preset,
		},
		builds::guide_user_to_select_profile,
		prompt::display_message,
	},
};
use clap::{Args, Parser};
use cliclack::spinner;
use frame_benchmarking_cli::OverheadCmd;
use pop_common::Profile;
use pop_parachains::{generate_omni_bencher_benchmarks, OmniBencherCommand};

const EXCLUDED_ARGS: [&str; 3] = ["--profile", "--skip-config", "-y"];

#[derive(Args)]
pub(crate) struct BenchmarkOverhead {
	/// Commmand to benchmark the execution overhead per-block and per-extrinsic.
	#[clap(flatten)]
	pub command: OverheadCmd,
	/// Build profile.
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl BenchmarkOverhead {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let spinner = spinner();
		cli.intro("Benchmarking the execution overhead per-block and per-extrinsic")?;

		if let Err(e) = self.interact(cli).await {
			return display_message(&e.to_string(), false, cli);
		};

		cli.warning("NOTE: this may take some time...")?;
		spinner.start("Benchmarking the execution overhead and generating weight file...");
		let result = self.run(cli).await;
		spinner.clear();

		// Display the benchmarking command.
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
				if self.profile.is_none() {
					self.profile = Some(guide_user_to_select_profile(cli)?);
				};
				cmd.params.runtime = Some(ensure_runtime_binary_exists(
					cli,
					&current_dir().unwrap_or(PathBuf::from("./")),
					self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
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

	async fn run(&self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let binary_path = check_omni_bencher_and_prompt(cli, self.skip_confirm).await?;
		generate_omni_bencher_benchmarks(
			binary_path.as_path(),
			OmniBencherCommand::Overhead,
			self.collect_arguments(),
			false,
		)?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut args = vec!["pop bench overhead".to_string()];
		let mut arguments = self.collect_arguments();
		if let Some(ref profile) = self.profile {
			arguments.push(format!("--profile={}", profile));
		}
		if self.skip_confirm {
			arguments.push("--skip-confirm".to_string());
		}
		args.extend(arguments);
		args.join(" ")
	}

	fn collect_arguments(&self) -> Vec<String> {
		let mut arguments: Vec<String> = std::env::args()
			.skip(3)
			// Exclude custom arguments which are not in the `OverheadCommand`.
			.filter(|arg| !EXCLUDED_ARGS.iter().any(|a| arg.starts_with(a)))
			.collect();

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
		arguments
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
	use strum::{EnumMessage, VariantArray};
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
			BenchmarkOverhead {
				command: OverheadCmd::try_parse_from([""]).unwrap(),
				skip_confirm: false,
				profile: Some(Profile::Debug)
			}
			.display(),
			"pop bench overhead --genesis-builder=runtime --genesis-builder-preset=development --profile=debug"
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
				.unwrap(),
				skip_confirm: true,
				profile: Some(Profile::Debug)
			}
			.display(),
			"pop bench overhead --runtime=dummy-runtime --genesis-builder=runtime \
			--genesis-builder-preset=development --weight-path=weights.rs --profile=debug --skip-confirm"
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
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_select(
				"Choose the build profile of the binary that should be used: ",
				Some(true),
				true,
				Some(profiles),
				0,
				None,
			)
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
			// Unable to mock the `std::env::args` for testing. In production, in must include
			// `--warmup` and `--repeat`.
			.expect_success(format!(
				"pop bench overhead --runtime={} --genesis-builder=runtime \
				--genesis-builder-preset=development --weight-path={} --profile=debug --skip-confirm",
				runtime_path.display(),
				output_path.to_string(),
			))
			.expect_outro("Benchmark completed successfully!");

		let cmd = OverheadCmd::try_parse_from(["", "--warmup=1", "--repeat=1"])?;
		assert!(BenchmarkOverhead { command: cmd, skip_confirm: true, profile: None }
			.execute(&mut cli)
			.await
			.is_ok());
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_overhead_invalid_weight_path_fails() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(true);
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
				"Failed to run benchmarking: Error: Input(\"Need directory as --weight-path\")",
			);
		let cmd = OverheadCmd::try_parse_from([
			"",
			"--runtime",
			get_mock_runtime(false).to_str().unwrap(),
			"--weight-path=weights.rs",
		])?;
		assert!(BenchmarkOverhead {
			command: cmd,
			skip_confirm: true,
			profile: Some(Profile::Debug)
		}
		.execute(&mut cli)
		.await
		.is_ok());
		cli.verify()
	}
}
