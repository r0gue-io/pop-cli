use std::path::PathBuf;

use clap::Args;
use cliclack::log;

use crate::engines::contract_engine::create_smart_contract;

#[derive(Args)]
pub struct NewContractCommand {
    #[arg(help = "Name of the contract")]
    pub(crate) name: String,
    #[arg(short = 'p', long = "path", help = "Path for the contract project, [default: current directory]")]
    pub(crate) path: Option<PathBuf>,
}

impl NewContractCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        create_smart_contract(self.name.clone(), &self.path)?;
        log::info(format!(
            "Smart contract created. Move to dir {:?}",
            self.path.clone().unwrap_or(PathBuf::from(format!("/{}", self.name))).display()
        ))?;
        Ok(())
    }
}