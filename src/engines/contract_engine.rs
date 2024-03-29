use anyhow::Context;
use cliclack::log;
use duct::cmd;
use std::path::PathBuf;

use contract_build::{
	execute, new_contract_project, BuildArtifacts, BuildMode, ExecuteArgs, Features, ManifestPath,
	Network, OptimizationPasses, OutputType, Target, UnstableFlags, Verbosity,
	DEFAULT_MAX_MEMORY_PAGES,
};
use contract_extrinsics::{CallExec, DisplayEvents, ErrorVariant, InstantiateExec, TokenMetadata};
use ink_env::DefaultEnvironment;
use sp_weights::Weight;
use subxt::PolkadotConfig as DefaultConfig;
use subxt_signer::sr25519::Keypair;

pub fn create_smart_contract(name: String, target: &Option<PathBuf>) -> anyhow::Result<()> {
	new_contract_project(&name, target.as_ref())
}

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
	// If the user specifies a path (which is not the current directory), it will have to manually add a Cargo.toml file. If not provided, pop-cli will ask the user for a specific path. or
	// ask to the user the specific path (Like cargo-contract does)
	let manifest_path;
	if path.is_some() {
		let full_path: PathBuf =
			PathBuf::from(path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml");
		manifest_path = ManifestPath::try_from(Some(full_path))?;
	} else {
		manifest_path = ManifestPath::try_from(path.as_ref())?;
	}

	let args = ExecuteArgs {
		manifest_path,
		verbosity: Verbosity::Default,
		build_mode: BuildMode::Release,
		features: Features::default(),
		network: Network::Online,
		build_artifact: BuildArtifacts::All,
		unstable_flags: UnstableFlags::default(),
		optimization_passes: Some(OptimizationPasses::default()),
		keep_debug_symbols: false,
		extra_lints: false,
		output_type: OutputType::Json,
		skip_wasm_validation: false,
		target: Target::Wasm,
		max_memory_pages: DEFAULT_MAX_MEMORY_PAGES,
		image: Default::default(),
	};

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();
	log::success(formatted_result.to_string())?;

	Ok(())
}

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
	use std::{fs, path::PathBuf};

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_contract_dir = tempfile::tempdir().expect("Could not create temp dir");
		let result: anyhow::Result<()> = create_smart_contract(
			"test_contract".to_string(),
			&Some(PathBuf::from(temp_contract_dir.path())),
		);

		assert!(result.is_ok(), "Result should be Ok");

		Ok(temp_contract_dir)
	}

	#[test]
	fn test_contract_create() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;

		// Verify that the generated smart contract contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_contract_dir.path().join("test_contract/lib.rs"))
				.expect("Could not read file");

		assert!(generated_file_content.contains("#[ink::contract]"));
		assert!(generated_file_content.contains("mod test_contract {"));

		// Verify that the generated Cargo.toml file contains the expected content
		fs::read_to_string(temp_contract_dir.path().join("test_contract/Cargo.toml"))
			.expect("Could not read file");
		Ok(())
	}

	#[test]
	fn test_contract_test() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;

		let result = test_smart_contract(&Some(temp_contract_dir.path().join("test_contract")));

		assert!(result.is_ok(), "Result should be Ok");
		Ok(())
	}
}
