use cliclack::log;
use duct::cmd;
use std::path::PathBuf;

use contract_build::{
	execute, new_contract_project, BuildArtifacts, BuildMode, ExecuteArgs, Features, ManifestPath,
	Network, OptimizationPasses, OutputType, Target, UnstableFlags, Verbosity,
	DEFAULT_MAX_MEMORY_PAGES,
};
use contract_extrinsics::{ErrorVariant, InstantiateExec};
use ink_env::DefaultEnvironment;
use sp_weights::Weight;
use subxt::PolkadotConfig as DefaultConfig;
use subxt_signer::sr25519::Keypair;

pub fn create_smart_contract(name: String, target: &Option<PathBuf>) -> anyhow::Result<()> {
	new_contract_project(&name, target.as_ref())
}

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
	// If the user specify a path (not current directory) have to manually add Cargo.toml here or
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

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempdir;

	#[test]
	fn test_create_smart_contract() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempdir::TempDir::new("test_folder")?;
		let result: anyhow::Result<()> =
			create_smart_contract("test".to_string(), &Some(PathBuf::from(temp_dir.path())));
		assert!(result.is_ok());

		// Verify that the generated smart contract contains the expected content
		let generated_file_content = fs::read_to_string(temp_dir.path().join("test/lib.rs"))?;

		assert!(generated_file_content.contains("#[ink::contract]"));
		assert!(generated_file_content.contains("mod test {"));

		Ok(())
	}
}
