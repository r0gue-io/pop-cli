use std::path::PathBuf;

use contract_build::new_contract_project;

pub fn create_smart_contract(name: String, target: PathBuf) -> anyhow::Result<()> {
    new_contract_project(&name, Some(target))
}