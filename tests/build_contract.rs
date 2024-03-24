use assert_cmd::Command;
use predicates::prelude::*;
use anyhow::{Result, Error};

fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
    let temp_contract_dir = tempfile::tempdir().unwrap();
	// pop new contract test_contract
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_contract_dir)
		.args(&["new", "contract", "test_contract"])
		.assert()
		.success();

    Ok(temp_contract_dir)
}

#[test]
fn test_contract_build() -> Result<(), Error> {
    let temp_contract_dir = setup_test_environment()?;

    // pop new contract test_contract
	Command::cargo_bin("pop")
        .unwrap()
        .current_dir(&temp_contract_dir.path().join("test_contract"))
        .args(&["build", "contract"])
        .assert()
        .success();

    // Verify that the folder target has been created
    assert!(temp_contract_dir.path().join("test_contract/target").exists());
    // Verify that all the artifacts has been generated
    assert!(temp_contract_dir.path().join("test_contract/target/ink/test_contract.contract").exists());
    assert!(temp_contract_dir.path().join("test_contract/target/ink/test_contract.wasm").exists());
    assert!(temp_contract_dir.path().join("test_contract/target/ink/test_contract.json").exists());

    Ok(())
}

#[test]
fn test_contract_build_specify_path() -> Result<(), Error> {
    let temp_contract_dir = setup_test_environment()?;

    // pop new contract test_contract
	Command::cargo_bin("pop")
        .unwrap()
        .current_dir(&temp_contract_dir.path())
        .args(&["build", "contract", "--path", "./test_contract"])
        .assert()
        .success();

    // Verify that the folder target has been created
    assert!(temp_contract_dir.path().join("test_contract/target").exists());
    // Verify that all the artifacts has been generated
    assert!(temp_contract_dir.path().join("test_contract/target/ink/test_contract.contract").exists());
    assert!(temp_contract_dir.path().join("test_contract/target/ink/test_contract.wasm").exists());
    assert!(temp_contract_dir.path().join("test_contract/target/ink/test_contract.json").exists());

    Ok(())
}

#[test]
fn test_contract_build_fails_if_no_contract_exists() -> Result<(), Error> {

    // pop new contract test_contract
	Command::cargo_bin("pop")
        .unwrap()
        .args(&["build", "contract",])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error: No 'ink' dependency found"));

    Ok(())
}
