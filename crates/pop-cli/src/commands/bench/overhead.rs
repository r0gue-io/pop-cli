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
	pub fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let cmd = &mut self.command;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

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

		if let Err(e) = generate_overhead_benchmarks(&cmd) {
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

	#[test]
	fn parse_genesis_builder_policy_works() -> anyhow::Result<()> {
		for policy in ["runtime", "spec-runtime", "spec-genesis"] {
			parse_genesis_builder_policy(policy)?;
		}
		Ok(())
	}
}
