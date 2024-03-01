use std::path::PathBuf;

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
