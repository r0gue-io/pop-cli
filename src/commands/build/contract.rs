use std::path::PathBuf;

use clap::Args;
use cliclack::log;

use crate::engines::contract_engine::build_smart_contract;

#[derive(Args)]
pub struct BuildContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
}

impl BuildContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		build_smart_contract(&self.path)?;
		log::info("The smart contract has been successfully built.")?;
		Ok(())
	}
}
