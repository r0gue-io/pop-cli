use std::path::PathBuf;

use clap::Args;
use cliclack::log;

use crate::engines::contract_engine::create_smart_contract;

#[derive(Args)]
pub struct NewContractCommand {
    #[arg(help = "Name of the contract")]
    pub(crate) name: String,
    #[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
    pub(crate) path: Option<PathBuf>,
}

impl NewContractCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        create_smart_contract(self.name.clone(), &self.path)?;
        log::info(format!(
            "Smart contract created! Located in the following directory {:?}",
            self.path.clone().unwrap_or(PathBuf::from(format!("/{}", self.name))).display()
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_new_contract_command_execute() -> anyhow::Result<()> {
        let command = NewContractCommand {
            name: "test_contract".to_string(),
            path: Some(PathBuf::new())
        };
        let result = command.execute();
        assert!(result.is_ok());
        
        // Clean up
        if let Err(err) = fs::remove_dir_all("test_contract") {
            eprintln!("Failed to delete directory: {}", err);
        }
        Ok(())
    }
}
