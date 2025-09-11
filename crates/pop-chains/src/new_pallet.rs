// SPDX-License-Identifier: GPL-3.0

use std::{
	fs::{create_dir, create_dir_all, File},
	path::{Path, PathBuf},
};

pub mod new_pallet_options;

use crate::{
	errors::Error,
	generator::pallet::{
		PalletAdvancedBenchmarking, PalletAdvancedLib, PalletAdvancedMock, PalletAdvancedTests,
		PalletCargoToml, PalletConfigPreludes, PalletItem, PalletLogic, PalletOrigin,
		PalletSimpleBenchmarking, PalletSimpleLib, PalletSimpleMock, PalletSimpleTests,
		PalletTestsUtils, PalletTryState, PalletTypes, PalletWeights,
	},
	utils::helpers::sanitize,
	TemplatePalletConfigCommonTypes, TemplatePalletStorageTypes,
};

/// Metadata for the Template Pallet.
#[derive(Debug)]
pub struct TemplatePalletConfig {
	/// The authors of the pallet
	pub authors: String,
	/// The pallet description
	pub description: String,
	/// Indicate if the pallet is contained in a workspace
	pub pallet_in_workspace: bool,
	/// Indicate if the user wanna use the advanced mode
	pub pallet_advanced_mode: bool,
	/// Indicate if the template must include a default config for the pallet.
	pub pallet_default_config: bool,
	/// Types defined in `TemplatePalletConfigCommonTypes` that should be included in the template.
	pub pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	/// Types defined in `TemplatePalletStorageTypes` that should be included in the template.
	pub pallet_storage: Vec<TemplatePalletStorageTypes>,
	/// Indicate if the template should include a genesis config
	pub pallet_genesis: bool,
	/// Indicate if the template should include a custom origin
	pub pallet_custom_origin: bool,
}
/// Create a new pallet from a template.
///
/// # Arguments
///
/// * `path` - location where the pallet will be created.
/// * `config` - customization values to include in the new pallet.
pub fn create_pallet_template(path: PathBuf, config: TemplatePalletConfig) -> Result<(), Error> {
	sanitize(&path)?;
	generate_pallet_structure(&path, &config)?;
	render_pallet(config, &path)?;
	Ok(())
}

/// Generate a pallet folder and file structure
fn generate_pallet_structure(path: &PathBuf, config: &TemplatePalletConfig) -> Result<(), Error> {
	create_dir_all(path)?;
	let (src, pallet_logic, tests) =
		(path.join("src"), path.join("src/pallet_logic"), path.join("src/tests"));
	create_dir(&src)?;
	File::create(format!("{}/Cargo.toml", path.display()))?;
	File::create(format!("{}/lib.rs", src.display()))?;
	File::create(format!("{}/benchmarking.rs", src.display()))?;
	File::create(format!("{}/tests.rs", src.display()))?;
	File::create(format!("{}/mock.rs", src.display()))?;
	if config.pallet_advanced_mode {
		create_dir(&pallet_logic)?;
		create_dir(&tests)?;
		File::create(format!("{}/pallet_logic.rs", src.display()))?;
		File::create(format!("{}/try_state.rs", pallet_logic.display()))?;
		File::create(format!("{}/types.rs", src.display()))?;
		File::create(format!("{}/utils.rs", tests.display()))?;
		if config.pallet_default_config {
			File::create(format!("{}/config_preludes.rs", src.display()))?;
		}
		if config.pallet_custom_origin {
			File::create(format!("{}/origin.rs", pallet_logic.display()))?;
		}
	} else {
		File::create(format!("{}/weights.rs", src.display()))?;
	}
	Ok(())
}

fn render_pallet(config: TemplatePalletConfig, pallet_path: &Path) -> Result<(), Error> {
	// Extract the pallet name from the path.
	let pallet_name = pallet_path
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or(Error::PathError)?
		.replace('-', "_");
	let mut pallet: Vec<Box<dyn PalletItem>> = vec![Box::new(PalletCargoToml {
		name: pallet_name.clone(),
		authors: config.authors,
		description: config.description,
		pallet_in_workspace: config.pallet_in_workspace,
		pallet_common_types: config.pallet_common_types.clone(),
	})];
	let mut pallet_contents: Vec<Box<dyn PalletItem>>;
	if config.pallet_advanced_mode {
		pallet_contents = vec![
			Box::new(PalletAdvancedLib {
				name: pallet_name.clone(),
				pallet_default_config: config.pallet_default_config,
				pallet_common_types: config.pallet_common_types.clone(),
				pallet_storage: config.pallet_storage.clone(),
				pallet_genesis: config.pallet_genesis,
				pallet_custom_origin: config.pallet_custom_origin,
			}),
			Box::new(PalletAdvancedTests {}),
			Box::new(PalletAdvancedMock {
				name: pallet_name.clone(),
				pallet_default_config: config.pallet_default_config,
				pallet_common_types: config.pallet_common_types.clone(),
				pallet_custom_origin: config.pallet_custom_origin,
			}),
			Box::new(PalletAdvancedBenchmarking {}),
			Box::new(PalletLogic { pallet_custom_origin: config.pallet_custom_origin }),
			Box::new(PalletTryState {}),
			Box::new(PalletTestsUtils { name: pallet_name.clone() }),
			Box::new(PalletTypes {
				pallet_common_types: config.pallet_common_types.clone(),
				pallet_storage: config.pallet_storage,
				pallet_custom_origin: config.pallet_custom_origin,
			}),
		];
		if config.pallet_default_config {
			pallet_contents.push(Box::new(PalletConfigPreludes {
				pallet_common_types: config.pallet_common_types,
				pallet_custom_origin: config.pallet_custom_origin,
			}));
		}

		if config.pallet_custom_origin {
			pallet_contents.push(Box::new(PalletOrigin {}));
		}
	} else {
		pallet_contents = vec![
			Box::new(PalletSimpleLib { name: pallet_name.clone() }),
			Box::new(PalletSimpleTests { name: pallet_name.clone() }),
			Box::new(PalletSimpleMock { name: pallet_name.clone() }),
			Box::new(PalletSimpleBenchmarking {}),
			Box::new(PalletWeights {}),
		];
	}

	pallet.extend(pallet_contents);

	for item in pallet {
		item.execute(pallet_path)?;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs::read_to_string;

	#[test]
	fn test_pallet_create_advanced_template() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let pallet_name = "MyPallet";
		let pallet_path = temp_dir.path().join(pallet_name);
		let config = TemplatePalletConfig {
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
			pallet_in_workspace: false,
			pallet_advanced_mode: true,
			pallet_default_config: true,
			pallet_common_types: Vec::new(),
			pallet_storage: Vec::new(),
			pallet_genesis: false,
			pallet_custom_origin: true,
		};

		// Call the function being tested
		create_pallet_template(pallet_path.clone(), config)?;

		// Assert that the pallet structure is generated
		assert!(pallet_path.exists(), "Pallet folder should be created");
		assert!(pallet_path.join("src").exists(), "src folder should be created");
		assert!(
			pallet_path.join("src").join("pallet_logic").exists(),
			"pallet_logic folder should be created"
		);
		assert!(
			pallet_path.join("src").join("pallet_logic").join("try_state.rs").exists(),
			"try_state.rs should be created"
		);
		assert!(
			pallet_path.join("src").join("pallet_logic").join("origin.rs").exists(),
			"origin.rs should be created"
		);
		assert!(pallet_path.join("src").join("tests").exists(), "tests folder should be created");
		assert!(
			pallet_path.join("src").join("tests").join("utils.rs").exists(),
			"utils.rs folder should be created"
		);
		assert!(pallet_path.join("Cargo.toml").exists(), "Cargo.toml should be created");
		assert!(pallet_path.join("src").join("lib.rs").exists(), "lib.rs should be created");
		assert!(
			pallet_path.join("src").join("benchmarking.rs").exists(),
			"benchmarking.rs should be created"
		);
		assert!(pallet_path.join("src").join("tests.rs").exists(), "tests.rs should be created");
		assert!(
			!pallet_path.join("src").join("weights.rs").exists(),
			"weights.rs shouldn't be created"
		);
		assert!(pallet_path.join("src").join("mock.rs").exists(), "mock.rs should be created");
		assert!(
			pallet_path.join("src").join("pallet_logic.rs").exists(),
			"pallet_logic.rs should be created"
		);
		assert!(
			pallet_path.join("src").join("config_preludes.rs").exists(),
			"config_preludes.rs should be created"
		);

		let lib_rs_content =
			read_to_string(pallet_path.join("src").join("lib.rs")).expect("Failed to read lib.rs");
		assert!(lib_rs_content.contains("pub mod pallet"), "lib.rs should contain pub mod pallet");
		assert!(
			lib_rs_content.contains("pub mod config_preludes"),
			"lib.rs should contain pub mod config_preludes"
		);
		Ok(())
	}

	#[test]
	fn test_pallet_create_advanced_template_no_default_config() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let pallet_name = "MyPallet";
		let pallet_path = temp_dir.path().join(pallet_name);
		let config = TemplatePalletConfig {
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
			pallet_in_workspace: false,
			pallet_advanced_mode: true,
			pallet_default_config: false,
			pallet_common_types: Vec::new(),
			pallet_storage: Vec::new(),
			pallet_genesis: false,
			pallet_custom_origin: true,
		};

		// Call the function being tested
		create_pallet_template(pallet_path.clone(), config)?;

		// Assert that the pallet structure is generated
		assert!(pallet_path.exists(), "Pallet folder should be created");
		assert!(pallet_path.join("src").exists(), "src folder should be created");
		assert!(
			pallet_path.join("src").join("pallet_logic").exists(),
			"pallet_logic folder should be created"
		);
		assert!(
			pallet_path.join("src").join("pallet_logic").join("try_state.rs").exists(),
			"try_state.rs should be created"
		);
		assert!(
			pallet_path.join("src").join("pallet_logic").join("origin.rs").exists(),
			"origin.rs should be created"
		);
		assert!(pallet_path.join("src").join("tests").exists(), "tests folder should be created");
		assert!(
			pallet_path.join("src").join("tests").join("utils.rs").exists(),
			"utils.rs folder should be created"
		);
		assert!(pallet_path.join("Cargo.toml").exists(), "Cargo.toml should be created");
		assert!(pallet_path.join("src").join("lib.rs").exists(), "lib.rs should be created");
		assert!(
			pallet_path.join("src").join("benchmarking.rs").exists(),
			"benchmarking.rs should be created"
		);
		assert!(
			!pallet_path.join("src").join("weights.rs").exists(),
			"weights.rs shouldn't be created"
		);
		assert!(pallet_path.join("src").join("tests.rs").exists(), "tests.rs should be created");
		assert!(pallet_path.join("src").join("mock.rs").exists(), "mock.rs should be created");
		assert!(
			pallet_path.join("src").join("pallet_logic.rs").exists(),
			"pallet_logic.rs should be created"
		);
		assert!(
			!pallet_path.join("src").join("config_preludes.rs").exists(),
			"config_preludes.rs should be created"
		);

		let lib_rs_content =
			read_to_string(pallet_path.join("src").join("lib.rs")).expect("Failed to read lib.rs");
		assert!(lib_rs_content.contains("pub mod pallet"), "lib.rs should contain pub mod pallet");
		assert!(
			!lib_rs_content.contains("pub mod config_preludes"),
			"lib.rs should contain pub mod config_preludes"
		);
		Ok(())
	}

	#[test]
	fn test_pallet_create_advanced_template_no_custom_origin() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let pallet_name = "MyPallet";
		let pallet_path = temp_dir.path().join(pallet_name);
		let config = TemplatePalletConfig {
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
			pallet_in_workspace: true,
			pallet_advanced_mode: true,
			pallet_default_config: true,
			pallet_common_types: Vec::new(),
			pallet_storage: Vec::new(),
			pallet_genesis: false,
			pallet_custom_origin: false,
		};

		// Call the function being tested
		create_pallet_template(pallet_path.clone(), config)?;

		// Assert that the pallet structure is generated
		assert!(pallet_path.exists(), "Pallet folder should be created");
		assert!(pallet_path.join("src").exists(), "src folder should be created");
		assert!(
			pallet_path.join("src").join("pallet_logic").exists(),
			"pallet_logic folder should be created"
		);
		assert!(
			pallet_path.join("src").join("pallet_logic").join("try_state.rs").exists(),
			"try_state.rs should be created"
		);
		assert!(
			!pallet_path.join("src").join("pallet_logic").join("origin.rs").exists(),
			"origin.rs should be created"
		);
		assert!(pallet_path.join("src").join("tests").exists(), "tests folder should be created");
		assert!(
			pallet_path.join("src").join("tests").join("utils.rs").exists(),
			"utils.rs folder should be created"
		);
		assert!(pallet_path.join("Cargo.toml").exists(), "Cargo.toml should be created");
		assert!(pallet_path.join("src").join("lib.rs").exists(), "lib.rs should be created");
		assert!(
			pallet_path.join("src").join("benchmarking.rs").exists(),
			"benchmarking.rs should be created"
		);
		assert!(
			!pallet_path.join("src").join("weights.rs").exists(),
			"weights.rs shouldn't be created"
		);
		assert!(pallet_path.join("src").join("tests.rs").exists(), "tests.rs should be created");
		assert!(pallet_path.join("src").join("mock.rs").exists(), "mock.rs should be created");
		assert!(
			pallet_path.join("src").join("pallet_logic.rs").exists(),
			"pallet_logic.rs should be created"
		);
		assert!(
			pallet_path.join("src").join("config_preludes.rs").exists(),
			"config_preludes.rs should be created"
		);

		let lib_rs_content =
			read_to_string(pallet_path.join("src").join("lib.rs")).expect("Failed to read lib.rs");
		assert!(lib_rs_content.contains("pub mod pallet"), "lib.rs should contain pub mod pallet");
		assert!(
			lib_rs_content.contains("pub mod config_preludes"),
			"lib.rs should contain pub mod config_preludes"
		);
		Ok(())
	}

	#[test]
	fn test_pallet_create_simple_template() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let pallet_name = "MyPallet";
		let pallet_path = temp_dir.path().join(pallet_name);
		let config = TemplatePalletConfig {
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
			pallet_in_workspace: false,
			pallet_advanced_mode: false,
			pallet_default_config: false,
			pallet_common_types: Vec::new(),
			pallet_storage: Vec::new(),
			pallet_genesis: false,
			pallet_custom_origin: false,
		};

		// Call the function being tested
		create_pallet_template(pallet_path.clone(), config)?;

		// Assert that the pallet structure is generated
		let pallet_path = temp_dir.path().join(pallet_name);
		assert!(pallet_path.exists(), "Pallet folder should be created");
		assert!(pallet_path.join("src").exists(), "src folder should be created");
		assert!(
			!pallet_path.join("src").join("pallet_logic").exists(),
			"pallet_logic folder shouldn't be created"
		);
		assert!(
			!pallet_path.join("src").join("pallet_logic").join("try_state.rs").exists(),
			"try_state.rs shouldn't be created"
		);
		assert!(
			!pallet_path.join("src").join("pallet_logic").join("origin.rs").exists(),
			"origin.rs shouldn't be created"
		);
		assert!(!pallet_path.join("src").join("tests").exists(), "tests folder should be created");
		assert!(
			!pallet_path.join("src").join("tests").join("utils.rs").exists(),
			"utils.rs folder shouldn't be created"
		);
		assert!(pallet_path.join("Cargo.toml").exists(), "Cargo.toml should be created");
		assert!(pallet_path.join("src").join("lib.rs").exists(), "lib.rs should be created");
		assert!(
			pallet_path.join("src").join("benchmarking.rs").exists(),
			"benchmarking.rs should be created"
		);
		assert!(
			pallet_path.join("src").join("weights.rs").exists(),
			"weights.rs should be created"
		);
		assert!(pallet_path.join("src").join("tests.rs").exists(), "tests.rs should be created");
		assert!(pallet_path.join("src").join("mock.rs").exists(), "mock.rs should be created");
		assert!(
			!pallet_path.join("src").join("pallet_logic.rs").exists(),
			"pallet_logic.rs shouldn't be created"
		);
		assert!(
			!pallet_path.join("src").join("config_preludes.rs").exists(),
			"config_preludes.rs shouldn't be created"
		);

		let lib_rs_content =
			read_to_string(pallet_path.join("src").join("lib.rs")).expect("Failed to read lib.rs");
		assert!(lib_rs_content.contains("pub mod pallet"), "lib.rs should contain pub mod pallet");
		assert!(
			!lib_rs_content.contains("pub mod config_preludes"),
			"lib.rs shouldn't contain pub mod config_preludes"
		);
		Ok(())
	}

	#[test]
	fn test_pallet_create_template_invalid_path() {
		// Use invalid characters that are not allowed in paths on any OS
		let invalid_path = "\0/\0"; // NULL byte is invalid in all filesystems
		let config = TemplatePalletConfig {
			authors: "Alice".to_string(),
			description: "A sample pallet".to_string(),
			pallet_in_workspace: false,
			pallet_advanced_mode: true,
			pallet_default_config: false,
			pallet_common_types: Vec::new(),
			pallet_storage: Vec::new(),
			pallet_genesis: false,
			pallet_custom_origin: false,
		};

		// Call the function being tested with an invalid path
		let result = create_pallet_template(PathBuf::from(invalid_path), config);

		// Assert that the result is an error
		assert!(result.is_err(), "Result should be an error");
	}
}
