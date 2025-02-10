// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::BuildSpecCommand,
	cli::{self},
};
use clap::Args;
use std::path::PathBuf;

const HELP_HEADER: &str = "Parachain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpParachainCommand {
	/// Path to the contract build directory.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short, long)]
	pub(crate) id: Option<u32>,
	/// Path to the genesis state file.
	#[arg(short = 'G', long = "genesis-state")]
	pub(crate) genesis_state: Option<PathBuf>,
	/// Path to the genesis code file.
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: Option<PathBuf>,
}

impl UpParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Deploy a parachain")?;
		let para_id = self.id.unwrap_or(reserve_para_id(cli)?);
		let (genesis_state, genesis_code) =
			match (self.genesis_state.clone(), self.genesis_code.clone()) {
				(Some(state), Some(code)) => (state, code),
				_ => generate_spec_files(para_id, cli).await?,
			};
		cli.outro("Parachain deployment complete.")?;
		Ok(())
	}
}

fn reserve_para_id(cli: &mut impl cli::traits::Cli) -> anyhow::Result<u32> {
	Ok(2000)
}

async fn generate_spec_files(
	id: u32,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<(PathBuf, PathBuf)> {
	let build_spec = BuildSpecCommand { id: Some(id), ..Default::default() }
		.configure_build_spec(cli)
		.await?;
	build_spec.generate_genesis_artifacts(cli)
}
