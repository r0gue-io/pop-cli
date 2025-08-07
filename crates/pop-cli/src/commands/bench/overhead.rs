// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Input},
	common::{
		bench::{check_omni_bencher_and_prompt, overwrite_weight_dir_command},
		builds::guide_user_to_select_profile,
		prompt::display_message,
		runtime::{ensure_runtime_binary_exists, guide_user_to_select_genesis_preset, Feature},
	},
};
use clap::{Args, Parser};
use cliclack::spinner;
use pop_chains::{bench::OverheadCmd, generate_omni_bencher_benchmarks, BenchmarkingCliCommand};
use pop_common::Profile;
use std::{env::current_dir, path::PathBuf};
use tempfile::tempdir;

const EXCLUDED_ARGS: [&str; 5] = ["--profile", "--skip-confirm", "-y", "--no-build", "-n"];

#[derive(Args)]
pub(crate) struct BenchmarkOverhead {
	/// Command to benchmark the execution overhead per-block and per-extrinsic.
	#[clap(flatten)]
	pub command: OverheadCmd,
	/// Build profile.
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// Avoid rebuilding the runtime if there is an existing runtime binary.
	#[clap(short = 'n', long)]
	no_build: bool,
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
		cli.info(self.display())?;
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
				let (binary_path, _) = ensure_runtime_binary_exists(
					cli,
					&current_dir().unwrap_or(PathBuf::from("./")),
					self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
					&[Feature::Benchmark],
					!self.no_build,
					false,
					&None,
				)?;
				cmd.params.runtime = Some(binary_path);
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

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let original_weight_path = self
			.command
			.params
			.weight
			.weight_path
			.clone()
			.ok_or_else(|| anyhow::anyhow!("No weight path provided"))?;

		if original_weight_path.is_file() {
			return Err(anyhow::anyhow!("Weight path needs to be a directory"));
		}
		self.command.params.weight.weight_path = Some(temp_dir.path().to_path_buf());

		let binary_path = check_omni_bencher_and_prompt(cli, self.skip_confirm).await?;
		let output = generate_omni_bencher_benchmarks(
			binary_path.as_path(),
			BenchmarkingCliCommand::Overhead,
			self.collect_arguments(),
			false,
		)?;
		println!("{}", output);

		// Restore the original weight path.
		self.command.params.weight.weight_path = Some(original_weight_path.clone());
		// Overwrite the weight files with the correct executed command.
		overwrite_weight_dir_command(
			temp_dir.path(),
			&original_weight_path,
			&self.collect_display_arguments(),
		)?;
		Ok(())
	}

	fn display(&self) -> String {
		self.collect_display_arguments().join(" ")
	}

	fn collect_display_arguments(&self) -> Vec<String> {
		let mut args = vec!["pop".to_string(), "bench".to_string(), "overhead".to_string()];
		let mut arguments = self.collect_arguments();
		if let Some(ref profile) = self.profile {
			arguments.push(format!("--profile={}", profile));
		}
		if self.skip_confirm {
			arguments.push("-y".to_string());
		}
		if self.no_build {
			arguments.push("-n".to_string());
		}
		args.extend(arguments);
		args
	}

	fn collect_arguments(&self) -> Vec<String> {
		let mut arguments: Vec<String> = std::env::args()
			.skip(3)
			// Exclude custom arguments which are not in the `OverheadCommand`.
			.filter(|arg| !EXCLUDED_ARGS.iter().any(|a| arg.starts_with(a)))
			.collect();

		#[cfg(test)]
		{
			arguments.retain(|arg| arg != "--show-output" && arg != "--nocapture");
		}
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
	use crate::{
		cli::MockCli,
		common::{
			bench::EXECUTED_COMMAND_COMMENT,
			runtime::{get_mock_runtime, Feature::Benchmark},
		},
	};
	use pop_chains::get_preset_names;
	use std::{
		env::current_dir,
		fs::{self, File},
		path::PathBuf,
	};
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
				profile: Some(Profile::Debug),
				no_build: false
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
				profile: Some(Profile::Debug),
				no_build: true
			}
			.display(),
			"pop bench overhead --runtime=dummy-runtime --genesis-builder=runtime \
			--genesis-builder-preset=development --weight-path=weights.rs --profile=debug \
			-y -n"
		);
	}

	#[tokio::test]
	async fn benchmark_overhead_works() -> anyhow::Result<()> {
		let cwd = current_dir().unwrap_or(PathBuf::from("./"));
		let temp_dir = tempdir()?;
		let output_path = temp_dir.path().to_str().unwrap();
		let runtime_path = get_mock_runtime(Some(Benchmark));
		let preset_names = get_preset_names(&runtime_path)?
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();

		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution overhead per-block and per-extrinsic")
			.expect_select(
				"Choose the build profile of the binary that should be used: ",
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				cwd.display()
			))
			.expect_input(
				"Please specify the path to the runtime project or the runtime binary.",
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
			.expect_info(format!(
				"pop bench overhead --runtime={} --genesis-builder=runtime \
				--genesis-builder-preset=development --weight-path={} --profile=debug -y",
				runtime_path.display(),
				output_path.to_string(),
			))
			.expect_outro("Benchmark completed successfully!");

		let cmd = OverheadCmd::try_parse_from(["", "--warmup=1", "--repeat=1"])?;
		assert!(BenchmarkOverhead {
			command: cmd,
			skip_confirm: true,
			profile: None,
			no_build: false
		}
		.execute(&mut cli)
		.await
		.is_ok());
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_overhead_weight_file_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime(Some(Benchmark));
		let output_path = temp_dir.path().to_str().unwrap();
		let preset_names = get_preset_names(&runtime_path)?
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
			.expect_input(
				"Provide the output directory path for weight files",
				output_path.to_string(),
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_outro("Benchmark completed successfully!");
		let mut cmd = BenchmarkOverhead {
			command: OverheadCmd::try_parse_from([
				"",
				&format!("--runtime={}", runtime_path.display()),
				"--warmup=1",
				"--repeat=1",
			])?,
			skip_confirm: true,
			profile: None,
			no_build: false,
		};
		assert!(cmd.execute(&mut cli).await.is_ok());

		for entry in temp_dir.path().read_dir()? {
			let path = entry?.path();
			if !path.is_file() {
				continue;
			}

			let mut command_block = format!("{EXECUTED_COMMAND_COMMENT}\n");
			for argument in cmd.collect_display_arguments() {
				command_block.push_str(&format!("//  {argument}\n"));
			}
			assert!(fs::read_to_string(temp_dir.path().join(path.file_name().unwrap()))?
				.contains(&command_block));
		}
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_overhead_invalid_weight_path_fails() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime(Some(Benchmark));
		let preset_names = get_preset_names(&runtime_path)?
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();

		File::create(temp_dir.path().join("weights.rs"))?;
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
			.expect_outro_cancel("Weight path needs to be a directory");
		let cmd = OverheadCmd::try_parse_from([
			"",
			"--runtime",
			get_mock_runtime(None).to_str().unwrap(),
			"--weight-path",
			temp_dir.path().join("weights.rs").to_str().unwrap(),
		])?;
		assert!(BenchmarkOverhead {
			command: cmd,
			skip_confirm: true,
			profile: Some(Profile::Debug),
			no_build: false
		}
		.execute(&mut cli)
		.await
		.is_ok());
		cli.verify()
	}
}
