use clap::Args;
use std::path::Path;
use contract_build::new_contract_project;
use crate::{
    engines::pallet_engine::{TemplatePalletConfig, create_pallet_template},
    helpers::{resolve_pallet_path},
};

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
        new_contract_project(&self.name, Some(target));
        Ok(())
    }
}