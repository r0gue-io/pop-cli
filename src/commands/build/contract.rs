use std::path::PathBuf;

use clap::Args;
use cliclack::log;

use crate::engines::contract_engine::build_smart_contract;

#[derive(Args)]
pub(crate) struct BuildContractCommand {
    #[arg(short = 'p', long = "path", help = "Path for the contract project, [default: current directory]")]
    path: Option<PathBuf>,
}

impl BuildContractCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        build_smart_contract(&self.path)?;
        log::info("Smart contract created")?;
        Ok(())
    }
}