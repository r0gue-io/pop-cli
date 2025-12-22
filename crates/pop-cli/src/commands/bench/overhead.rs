// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self, spinner,
		traits::{Input, Spinner},
	},
	common::{
		bench::{check_omni_bencher_and_prompt, overwrite_weight_dir_command},
		builds::guide_user_to_select_profile,
		prompt::display_message,
		runtime::{Feature, ensure_runtime_binary_exists, guide_user_to_select_genesis_preset},
	},
};
use clap::{Args, Parser};
use pop_chains::{BenchmarkingCliCommand, bench::OverheadCmd, generate_omni_bencher_benchmarks};
use pop_common::Profile;
use serde::Serialize;
use std::{env::current_dir, path::PathBuf};
use tempfile::tempdir;

const EXCLUDED_ARGS: [&str; 5] = ["--profile", "--skip-confirm", "-y", "--no-build", "-n"];

#[derive(Args, Serialize)]
pub(crate) struct BenchmarkOverhead {
	/// Command to benchmark the execution overhead per-block and per-extrinsic.
	#[serde(skip_serializing)]
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
	pub(crate) async fn execute(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<serde_json::Value> {
		let spinner = spinner();
		cli.intro("Benchmarking the execution overhead per-block and per-extrinsic")?;

		if let Err(e) = self.interact(cli).await {
			display_message(&e.to_string(), false, cli)?;
			return Err(e);
		};

		cli.warning("NOTE: this may take some time...")?;
		spinner.start("Benchmarking the execution overhead and generating weight file...");
		let result = self.run(cli).await;
		spinner.clear();

		// Display the benchmarking command.
		cli.info(self.display())?;
		let output = match result {
			Ok(output) => {
				display_message("Benchmark completed successfully!", true, cli)?;
				output
			},
			Err(e) => {
				display_message(&e.to_string(), false, cli)?;
				return Err(e);
			},
		};
		Ok(serde_json::to_value(crate::common::output::SuccessData { message: output })?)
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
					None,
				)
				.await?;
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
				if !self.skip_confirm {
					cmd.params.genesis_builder_preset = guide_user_to_select_genesis_preset(
						cli,
						runtime_path,
						&cmd.params.genesis_builder_preset,
					)?;
				}
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

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<String> {
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

		let spinner = spinner();
		let binary_path = check_omni_bencher_and_prompt(cli, &spinner, self.skip_confirm).await?;
		spinner.clear();
		let output = generate_omni_bencher_benchmarks(
			binary_path.as_path(),
			BenchmarkingCliCommand::Overhead,
			self.collect_arguments(),
			false,
		)?;
		if cli.is_json() {
			eprintln!("{}", output);
		} else {
			println!("{}", output);
		}

		// Restore the original weight path.
		self.command.params.weight.weight_path = Some(original_weight_path.clone());
		// Overwrite the weight files with the correct executed command.
		overwrite_weight_dir_command(
			temp_dir.path(),
			&original_weight_path,
			&self.collect_display_arguments(),
		)?;
		Ok(output)
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

		if print_runtime && let Some(ref runtime) = self.command.params.runtime {
			arguments.push(format!("--runtime={}", runtime.display()));
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
		if print_weight_path && let Some(ref weight_path) = self.command.params.weight.weight_path {
			arguments.push(format!("--weight-path={}", weight_path.display()));
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
	.map_err(|e| anyhow::anyhow!(format!(r#"Invalid genesis builder option {policy}: {e}"#)))
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
