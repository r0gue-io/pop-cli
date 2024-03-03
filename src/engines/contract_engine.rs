use std::path::PathBuf;
use duct::cmd;

use contract_build::{
    new_contract_project, execute, 
    ExecuteArgs, ManifestPath,Verbosity, BuildMode,Features,Network,BuildArtifacts, UnstableFlags, OptimizationPasses, OutputType, Target,
};

pub fn create_smart_contract(name: String, target: &Option<PathBuf>) -> anyhow::Result<()> {
    new_contract_project(&name, target.as_ref())
}

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
    // If the user specify a path (not current directory) have to manually add Cargo.toml here or ask to the user the specific path
    let manifest_path ;
    if path.is_some(){
        let full_path: PathBuf = PathBuf::from(path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml");
        manifest_path = ManifestPath::try_from(Some(full_path))?;
    }
    else {
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
        lint: false,
        output_type: OutputType::Json,
        skip_wasm_validation: false,
        target: Target::Wasm,
    };
    execute(args)?;
    Ok(())
}


pub fn test_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
    cmd(
        "cargo",
        vec![
            "test",
        ],
    )
    .dir(path.clone().unwrap_or(PathBuf::new()))
    .run()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir;
    use std::fs;

    #[test]
    fn test_create_smart_contract() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir::TempDir::new("test_folder")?;
        let result: anyhow::Result<()> = create_smart_contract("test".to_string(),&Some(PathBuf::from(temp_dir.path())));
        assert!(result.is_ok());

        // Verify that the generated smart contract contains the expected content
        let generated_file_content =
            fs::read_to_string(temp_dir.path().join("test/lib.rs"))?;
        
        assert!(generated_file_content.contains("#[ink::contract]"));
        assert!(generated_file_content.contains("mod test {"));
        
        Ok(())
    }
}