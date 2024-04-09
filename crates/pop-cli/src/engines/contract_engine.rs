use anyhow::Context;
use cliclack::log;
use duct::cmd;
use std::path::{Path, PathBuf};

use contract_build::{
	execute, BuildArtifacts, BuildMode, ExecuteArgs, Features, ManifestPath, Network,
	OptimizationPasses, OutputType, Target, UnstableFlags, Verbosity, DEFAULT_MAX_MEMORY_PAGES,
};
use contract_extrinsics::{CallExec, DisplayEvents, ErrorVariant, InstantiateExec, TokenMetadata};
use ink_env::DefaultEnvironment;
use sp_weights::Weight;
use subxt::PolkadotConfig as DefaultConfig;
use subxt_signer::sr25519::Keypair;

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

pub async fn instantiate_smart_contract(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<String, ErrorVariant> {
	let instantiate_result = instantiate_exec.instantiate(Some(gas_limit)).await?;
	Ok(instantiate_result.contract_address.to_string())
}

pub async fn dry_run_gas_estimate_instantiate(
	instantiate_exec: &InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<Weight> {
	let instantiate_result = instantiate_exec.instantiate_dry_run().await?;
	match instantiate_result.result {
        Ok(_) => {
            // use user specified values where provided, otherwise use the estimates
            let ref_time = instantiate_exec
                .args()
                .gas_limit()
                .unwrap_or_else(|| instantiate_result.gas_required.ref_time());
            let proof_size = instantiate_exec
                .args()
                .proof_size()
                .unwrap_or_else(|| instantiate_result.gas_required.proof_size());
            Ok(Weight::from_parts(ref_time, proof_size))
        }
        Err(ref _err) => {
             Err(anyhow::anyhow!(
                "Pre-submission dry-run failed. Add gas_limit and proof_size manually to skip this step."
            ))
        }
    }
}

pub async fn call_smart_contract(
	call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
	token_metadata: TokenMetadata,
) -> anyhow::Result<String, ErrorVariant> {
	let metadata = call_exec.client().metadata();
	let events = call_exec.call(Some(gas_limit)).await?;
	let display_events =
		DisplayEvents::from_events::<DefaultConfig, DefaultEnvironment>(&events, None, &metadata)?;

	let output =
		display_events.display_events::<DefaultEnvironment>(Verbosity::Default, &token_metadata)?;
	Ok(output)
}

pub async fn dry_run_gas_estimate_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<Weight> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
        Ok(_) => {
            // use user specified values where provided, otherwise use the estimates
            let ref_time = call_exec
                .gas_limit()
                .unwrap_or_else(|| call_result.gas_required.ref_time());
            let proof_size = call_exec
                .proof_size()
                .unwrap_or_else(|| call_result.gas_required.proof_size());
            Ok(Weight::from_parts(ref_time, proof_size))
        }
        Err(ref _err) => {
             Err(anyhow::anyhow!(
                "Pre-submission dry-run failed. Add gas_limit and proof_size manually to skip this step."
            ))
        }
    }
}

pub async fn dry_run_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<String> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
        Ok(ref ret_val) => {
            let value = call_exec
				.transcoder()
				.decode_message_return(
					call_exec.message(),
					&mut &ret_val.data[..],
				)
				.context(format!(
					"Failed to decode return value {:?}",
					&ret_val
			))?;
			Ok(value.to_string())
        }
        Err(ref _err) => {
             Err(anyhow::anyhow!(
                "Pre-submission dry-run failed. Add gas_limit and proof_size manually to skip this step."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use pop_contracts::create_smart_contract;
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		let result =
			create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

		Ok(temp_dir)
	}

	#[cfg(feature = "unit_contract")]
	#[test]
	fn test_contract_test() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;

		let result = test_smart_contract(&Some(temp_contract_dir.path().join("test_contract")));

		assert!(result.is_ok(), "Result should be Ok");

		Ok(())
	}
}
