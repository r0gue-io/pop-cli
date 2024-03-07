use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen,intro};

use crate::style::style;
use crate::engines::contract_engine::test_smart_contract;
use crate::engines::contract_engine::test_e2e_smart_contract;

#[derive(Args)]
pub(crate) struct TestContractCommand {
    #[arg(short = 'p', long = "path", help = "Path for the contract project [default: current directory]")]
    path: Option<PathBuf>,
    #[arg(short = 'f', long = "features", help = "Features for the contract project")]
    features: Option<String>,
}

impl TestContractCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        clear_screen()?;
        if self.features.is_some() && self.features.clone().unwrap().contains("e2e-tests") {
            intro(format!(
                "{}: Starting e2e tests",
                style(" Pop CLI ").black().on_magenta()
            ))?;
            test_e2e_smart_contract(&self.path)?;
        } else {
            intro(format!(
                "{}: Starting unit tests",
                style(" Pop CLI ").black().on_magenta()
            ))?;
            test_smart_contract(&self.path)?;

        }
        Ok(())
    }
}