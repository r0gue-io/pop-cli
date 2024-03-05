use std::path::PathBuf;

use clap::Args;
use cliclack::intro;

use crate::style::style;
// use crate::engines::contract_engine::create_smart_contract;

#[derive(Args)]
pub struct UpContractCommand {
    #[arg(short = 'p', long = "path", help = "Path for the contract project, [default: current directory]")]
    pub(crate) path: Option<PathBuf>,
}

impl UpContractCommand {
    pub(crate) async fn execute(&self) -> anyhow::Result<()> {
        intro(format!(
            "{}: Deploy a smart contract",
            style(" Pop CLI ").black().on_magenta()
        ))?;
        //create_smart_contract(self.name.clone(), &self.path)?;
        Ok(())
    }
}
