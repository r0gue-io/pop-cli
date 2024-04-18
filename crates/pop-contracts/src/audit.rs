use cargo_scout_audit::startup::{run_scout, OutputFormat, Scout};
use std::path::PathBuf;

use crate::utils::helpers::get_manifest_path;

pub fn audit_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<()> {
	let manifest_path = get_manifest_path(path)?;
	// Default values
    let scout_config = Scout {
        manifest_path: path.clone(),
        verbose: true,
        ..Default::default()
    };

	// Execute the build and log the output of the build
	let a = run_scout(scout_config)?;
    println!("{:?}", a);

	Ok(())
}