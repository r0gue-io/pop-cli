use clap::Args;
use cliclack::log;

use crate::{engines::contract_engine::create_smart_contract,helpers::resolve_pallet_path};

#[derive(Args)]
pub struct NewContractCommand {
    #[arg(help = "Name of the contract")]
    pub(crate) name: String,
    #[arg(short = 'p', long = "path", help = "Path for th contract project, [default: current directory]")]
    pub(crate) path: Option<String>,
}

impl NewContractCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        let target = resolve_pallet_path(self.path.clone());
        create_smart_contract(self.name.clone(), target)?;
        log::info(format!(
            "Smart contract created at {}",
            self.path.clone().unwrap_or(format!("folder {}",self.name).to_string())
        ))?;
        Ok(())
    }
}