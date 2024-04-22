// SPDX-License-Identifier: GPL-3.0
use std::{fs, path::PathBuf};

use crate::errors::Error;
use crate::{
	generator::pallet::{
		PalletBenchmarking, PalletCargoToml, PalletItem, PalletLib, PalletMock, PalletTests,
	},
	resolve_pallet_path,
	utils::helpers::sanitize,
};

pub struct TemplatePalletConfig {
	pub name: String,
	pub authors: String,
	pub description: String,
}

pub fn create_pallet_template(
	path: Option<String>,
	config: TemplatePalletConfig,
) -> Result<(), Error> {
	let target = resolve_pallet_path(path)?;
	let pallet_name = config.name.clone();
	let pallet_path = target.join(pallet_name.clone());
	sanitize(&pallet_path)?;
	generate_pallet_structure(&target, &pallet_name)?;

	render_pallet(pallet_name, config, &pallet_path)?;
	Ok(())
}

/// Generate a pallet folder and file structure
fn generate_pallet_structure(target: &PathBuf, pallet_name: &str) -> Result<(), Error> {
	use fs::{create_dir, File};
	let (pallet, src) = (target.join(pallet_name), target.join(pallet_name.to_string() + "/src"));
	create_dir(&pallet)?;
	create_dir(&src)?;
	File::create(format!("{}/Cargo.toml", pallet.display()))?;
	File::create(format!("{}/lib.rs", src.display()))?;
	File::create(format!("{}/benchmarking.rs", src.display()))?;
	File::create(format!("{}/tests.rs", src.display()))?;
	File::create(format!("{}/mock.rs", src.display()))?;
	Ok(())
}

fn render_pallet(
	pallet_name: String,
	config: TemplatePalletConfig,
	pallet_path: &PathBuf,
) -> Result<(), Error> {
	let pallet_name = pallet_name.replace('-', "_");
	// Todo `module` must be of the form Template if pallet_name : `pallet_template`
	let pallet: Vec<Box<dyn PalletItem>> = vec![
		Box::new(PalletCargoToml {
			name: pallet_name.clone(),
			authors: config.authors,
			description: config.description,
		}),
		Box::new(PalletLib {}),
		Box::new(PalletBenchmarking {}),
		Box::new(PalletMock { module: pallet_name.clone() }),
		Box::new(PalletTests { module: pallet_name }),
	];
	for item in pallet {
		item.execute(pallet_path)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_pallet_create_template() {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let pallet_name = "MyPallet";
		let config = TemplatePalletConfig {
			name: pallet_name.to_string(),
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
		};

		// Call the function being tested
		let result =
			create_pallet_template(Some(temp_dir.path().to_str().unwrap().to_string()), config);

		// Assert that the result is Ok
		assert!(result.is_ok(), "Result should be Ok");

		// Assert that the pallet structure is generated
		let pallet_path = temp_dir.path().join(pallet_name);
		assert!(pallet_path.exists(), "Pallet folder should be created");
		assert!(pallet_path.join("src").exists(), "src folder should be created");
		assert!(pallet_path.join("Cargo.toml").exists(), "Cargo.toml should be created");
		assert!(pallet_path.join("src").join("lib.rs").exists(), "lib.rs should be created");
		assert!(
			pallet_path.join("src").join("benchmarking.rs").exists(),
			"benchmarking.rs should be created"
		);
		assert!(pallet_path.join("src").join("tests.rs").exists(), "tests.rs should be created");
		assert!(pallet_path.join("src").join("mock.rs").exists(), "mock.rs should be created");

		let lib_rs_content = fs::read_to_string(pallet_path.join("src").join("lib.rs"))
			.expect("Failed to read lib.rs");
		assert!(lib_rs_content.contains("pub mod pallet"), "lib.rs should contain pub mod pallet");
	}

	#[test]
	fn test_pallet_create_template_invalid_path() {
		let invalid_path = "/invalid/path/that/does/not/exist";
		let pallet_name = "MyPallet";
		let config = TemplatePalletConfig {
			name: pallet_name.to_string(),
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
		};

		// Call the function being tested with an invalid path
		let result = create_pallet_template(Some(invalid_path.to_string()), config);

		// Assert that the result is an error
		assert!(result.is_err(), "Result should be an error");
	}
}
