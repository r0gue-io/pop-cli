use duct::cmd;
use std::path::PathBuf;

pub fn test_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
	cmd("cargo", vec!["test"]).dir(path.clone().unwrap_or("./".into())).run()?;

	Ok(())
}

pub fn test_e2e_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
	cmd("cargo", vec!["test", "--features=e2e-tests"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}

#[cfg(feature = "unit_contract")]
#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		let result =
			crate::create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

		Ok(temp_dir)
	}

	#[test]
	fn test_contract_test() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;

		let result = test_smart_contract(&Some(temp_contract_dir.path().join("test_contract")));

		assert!(result.is_ok(), "Result should be Ok");

		Ok(())
	}
}
