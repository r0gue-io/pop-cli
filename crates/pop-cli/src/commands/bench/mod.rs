// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::Cli};
use clap::{Args, Subcommand};
use frame_benchmarking_cli::PalletCmd;
use sp_runtime::traits::BlakeTwo256;

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Arguments for bencharmking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the extrinsic weight of FRAME Pallets
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Pallet(PalletCmd),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;
		match args.command {
			#[cfg(feature = "parachain")]
			Command::Pallet(cmd) => Command::bechmark_pallet(cmd, &mut cli),
		}
	}

	fn bechmark_pallet(cmd: PalletCmd, cli: &mut impl Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking your pallets")?;

		if let Some(spec) = cmd.shared_params.chain {
			return Err(anyhow::anyhow!(format!(
				"Chain specs are not supported. Please remove `--chain={spec}` and use \
								`--runtime=<PATH>` instead"
			)))?;
		}

		cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
			.map_err(|_| anyhow::anyhow!(format!("Failed to run benchmarking for the pallet")))?;
		Ok(())
	}
}
