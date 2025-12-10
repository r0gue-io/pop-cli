// SPDX-License-Identifier: GPL-3.0

use crate::{
	BuildInfo, BuildMode, CARGO_CONTRACT_VERSION, CodeHash, ContractMetadata, Error, ImageVariant,
	MetadataArtifacts, Verbosity,
};
use contract_build::BuildResult;
use regex::Regex;
use serde::Serialize;
use std::{fs::File, path::Path};

#[cfg_attr(test, derive(Debug))]
pub(super) struct BuildInfoParsed {
	pub(super) build_info: BuildInfo,
	pub(super) build_mode: BuildMode,
	pub(super) image: Option<ImageVariant>,
	pub(super) polkavm_blob: Vec<u8>,
}

/// Get the `BuildInfo` and `BuildMode` used to compile a contract bundle by inspecting the
/// `.contract` artifact
///
/// # Arguments
/// - `contract_bundle` - The `Path` to the ".contract" file
pub(super) fn get_build_info_parsed_from_contract_bundle(
	contract_bundle: &Path,
) -> Result<BuildInfoParsed, Error> {
	let file = File::open(contract_bundle)?;

	let metadata: ContractMetadata = serde_json::from_reader(&file)?;

	let polkavm_blob = contract_build::util::decode_hex(
		&metadata
			.source
			.contract_binary
			.ok_or(Error::ContractMetadata("No source binary present in metadata.".to_owned()))?
			.to_string(),
	)
	.expect("If a contract binary is present, it must be hex decodable. This is because ContractMetadata deserializer enforces this field is a valid byte str, so if it's not, this step isn't even reached; qed");

	if let Some(info) = metadata.source.build_info {
		let build_info: BuildInfo = serde_json::from_value(info.into())?;
		match metadata.image {
			Some(image) => Ok(BuildInfoParsed {
				build_info,
				build_mode: BuildMode::Verifiable,
				image: Some(ImageVariant::from(Some(image))),
				polkavm_blob,
			}),
			_ => {
				let build_mode = build_info.build_mode.clone();
				Ok(BuildInfoParsed { build_info, build_mode, image: None, polkavm_blob })
			},
		}
	} else {
		Err(Error::ContractMetadata("The metadata does not contain any build information which can be used to verify a contract.".to_owned()))
	}
}

/// Compare the local toolchain used in the operating system with the one specified in a concrete
/// `BuildInfo`
///
/// # Arguments
/// - `build_info` - The `BuildInfo` compared to the local toolchain
pub(super) fn compare_local_toolchain(build_info: &BuildInfo) -> Result<(), Error> {
	let expected_rust_toolchain = &build_info.rust_toolchain;
	let rust_toolchain = contract_build::util::rust_toolchain()
		.expect("`rustc` always has a version associated with it.");

	validate_toolchain_name(expected_rust_toolchain)?;

	if rust_toolchain != *expected_rust_toolchain {
		return Err(Error::InvalidToolchain(
			"You are trying to `verify` a contract using the following toolchain:\n\
                {rust_toolchain}\n\n\
                However, the original contract was built using this one:\n\
                {expected_rust_toolchain}\n\n\
                Please install the correct toolchain and re-run the `verify` command:\n\
                rustup install {expected_rust_toolchain}"
				.to_owned(),
		));
	}

	if build_info.cargo_contract_version != *CARGO_CONTRACT_VERSION {
		return Err(Error::InvalidToolchain(format!(
			"\nYou are trying to `verify` a contract using `cargo-contract` version \
                `{}`.\n\n\
                However, the original contract was built using `cargo-contract` version \
                `{}`.\n\n\". The cargo contract version is implied by the `pop` version used.",
			*CARGO_CONTRACT_VERSION, build_info.cargo_contract_version,
		)));
	}

	Ok(())
}

/// Verifies that a PolkaVM blob matches the contract binary in a build result.
///
/// This function computes the hash of the provided PolkaVM blob and compares it against
/// the hash stored in the contract metadata of the build result. This ensures that the
/// contract binary being verified matches what was actually built.
///
/// # Arguments
/// - `polkavm_blob` - The PolkaVM bytecode blob to verify
/// - `build_result` - The build result containing the contract metadata to compare against
pub(super) fn verify_polkavm_blob_against_build_result(
	polkavm_blob: &[u8],
	build_result: BuildResult,
) -> Result<(), Error> {
	let reference_polkavm_hash = CodeHash(contract_build::code_hash(polkavm_blob));
	let built_contract_path = if let Some(MetadataArtifacts::Ink(artifacts)) =
		build_result.metadata_result
	{
		artifacts
	} else {
		return Err(Error::ContractMetadata("The metadata for the workspace contract does not contain a contract binary, therefore we are unable to verify the contract.".to_owned()))
	};

	let target_bundle = &built_contract_path.dest_bundle;

	let file = File::open(target_bundle.clone())?;
	let built_contract: ContractMetadata = serde_json::from_reader(file)?;

	if reference_polkavm_hash != built_contract.source.hash {
		return Err(Error::Verification(format!(
			"Failed to verify the polkavm blob against the build result at {:?}.",
			target_bundle
		)));
	}

	Ok(())
}

/// Validates that the passed `toolchain` is a valid Rust toolchain.
///
/// # Developers Note
///
/// Strictly speaking Rust has not yet defined rules for legal toolchain
/// names. See https://github.com/rust-lang/rustup/issues/4059 for more
/// details.
///
/// We took a "good enough" approach and restrict valid toolchain names
/// to established ones.
fn validate_toolchain_name(toolchain: &str) -> Result<(), Error> {
	let re = Regex::new(r"^[a-zA-Z._\-0-9]+$").expect("failed creating regex");
	if re.is_match(toolchain) {
		Ok(())
	} else {
		Err(Error::InvalidToolchain(format!("Invalid toolchain name: {toolchain}")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use contract_build::{
		BuildArtifacts, OutputType,
		metadata::{InkMetadataArtifacts, MetadataArtifacts},
	};
	use std::{io::ErrorKind, path::PathBuf};
	use tempfile::TempDir;

	struct TestBuilder {
		temp_dir: TempDir,
		temp_files: Vec<PathBuf>,
	}

	impl TestBuilder {
		fn temp_path(&self) -> &Path {
			self.temp_dir.path()
		}

		fn execute<F>(self, test: F)
		where
			F: Fn(Self),
		{
			test(self)
		}
	}

	impl Default for TestBuilder {
		fn default() -> Self {
			Self { temp_dir: tempfile::tempdir().unwrap(), temp_files: Vec::new() }
		}
	}

	impl TestBuilder {
		fn load_test_file(mut self, test_file: &str) -> Self {
			let test_file_path = Path::new(test_file);
			// std::env::current_dir points to the crate root
			let test_files = std::env::current_dir().unwrap().join("tests").join("files");
			let source = test_files.join(test_file_path);
			let to = self.temp_path().join(test_file_path);
			std::fs::copy(&source, &to).unwrap();
			self.temp_files.push(to);
			self
		}
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_detects_verifiable() {
		TestBuilder::default().load_test_file("testing_verified.contract").execute(
			|mut builder| {
				let bundle_path = builder.temp_files.pop().unwrap();
				let result = get_build_info_parsed_from_contract_bundle(&bundle_path).unwrap();
				assert_eq!(BuildMode::Verifiable, result.build_mode);
				assert_eq!("stable-x86_64-unknown-linux-gnu", result.build_info.rust_toolchain);
				assert_eq!("6.0.0-beta.1", result.build_info.cargo_contract_version.to_string());
				assert!(result.image.is_some());
				assert!(!result.polkavm_blob.is_empty());
			},
		);
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_detects_non_verifiable() {
		TestBuilder::default()
			.load_test_file("testing.contract")
			.execute(|mut builder| {
				let bundle_path = builder.temp_files.pop().unwrap();
				let result = get_build_info_parsed_from_contract_bundle(&bundle_path).unwrap();
				assert_eq!(BuildMode::Release, result.build_mode);
				assert_eq!("stable-aarch64-apple-darwin", result.build_info.rust_toolchain);
				assert_eq!("6.0.0-beta.1", result.build_info.cargo_contract_version.to_string());
				assert!(result.image.is_none());
				assert!(!result.polkavm_blob.is_empty());
			});
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_file_not_found() {
		let result =
			get_build_info_parsed_from_contract_bundle(Path::new("/nonexistent/path.contract"));
		assert!(matches!(result, Err(Error::IO(err)) if err.kind() == ErrorKind::NotFound));
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_invalid_json() {
		TestBuilder::default()
			.load_test_file("testing.contract")
			.execute(|mut builder| {
				let bundle_path = builder.temp_files.pop().unwrap();
				// Corrupt the JSON file
				std::fs::write(&bundle_path, "not valid json").unwrap();
				let result = get_build_info_parsed_from_contract_bundle(&bundle_path);
				assert!(
					matches!(result, Err(Error::SerdeJson(err)) if err.to_string() == "expected ident at line 1 column 2")
				);
			});
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_missing_source_binary() {
		TestBuilder::default().load_test_file("testing.contract").execute(|mut builder|{
            let bundle_path = builder.temp_files.pop().unwrap();
            // Read and modify the metadata to remove contract_binary
            let file = File::open(&bundle_path).unwrap();
            let mut metadata: serde_json::Value = serde_json::from_reader(&file).unwrap();
            // Remove contract_binary from source
            metadata["source"].as_object_mut().unwrap().remove("contract_binary");
            std::fs::write(&bundle_path, serde_json::to_string(&metadata).unwrap()).unwrap();

            let result = get_build_info_parsed_from_contract_bundle(&bundle_path);
            assert!(matches!(result, Err(Error::ContractMetadata(msg)) if msg == "No source binary present in metadata."));
        });
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_missing_build_info() {
		TestBuilder::default().load_test_file("testing.contract").execute(|mut builder|{
            let bundle_path = builder.temp_files.pop().unwrap();
            // Read and modify the metadata to remove build_info
            let file = File::open(&bundle_path).unwrap();
            let mut metadata: serde_json::Value = serde_json::from_reader(&file).unwrap();
            // Remove build_info from source
            metadata["source"].as_object_mut().unwrap().remove("build_info");
            std::fs::write(&bundle_path, serde_json::to_string(&metadata).unwrap()).unwrap();

            let result = get_build_info_parsed_from_contract_bundle(&bundle_path);
            assert!(matches!(result, Err(Error::ContractMetadata(msg)) if msg == "The metadata does not contain any build information which can be used to verify a contract."));
        });
	}

	#[test]
	fn get_build_info_parsed_from_contract_bundle_invalid_build_info() {
		TestBuilder::default()
			.load_test_file("testing.contract")
			.execute(|mut builder| {
				let bundle_path = builder.temp_files.pop().unwrap();
				// Read and modify the metadata to corrupt build_info
				let file = File::open(&bundle_path).unwrap();
				let mut metadata: serde_json::Value = serde_json::from_reader(&file).unwrap();
				// Replace build_info with an empty object
				metadata["source"]["build_info"] = serde_json::json!({});
				std::fs::write(&bundle_path, serde_json::to_string(&metadata).unwrap()).unwrap();

				let result = get_build_info_parsed_from_contract_bundle(&bundle_path);
				assert!(
					matches!(result, Err(Error::SerdeJson(err)) if err.to_string() == "missing field `rust_toolchain`")
				);
			});
	}

	#[test]
	fn validate_toolchain_name_accepts_valid_toolchains() {
		// Test common valid toolchain names
		assert!(validate_toolchain_name("stable").is_ok());
		assert!(validate_toolchain_name("nightly").is_ok());
		assert!(validate_toolchain_name("1.70.0").is_ok());
		assert!(validate_toolchain_name("stable-x86_64-unknown-linux-gnu").is_ok());
		assert!(validate_toolchain_name("nightly-2023-01-01").is_ok());
		assert!(validate_toolchain_name("1.70.0-x86_64-apple-darwin").is_ok());
	}

	#[test]
	fn validate_toolchain_name_rejects_invalid_toolchains() {
		// Test invalid toolchain names with special characters
		assert!(matches!(
			validate_toolchain_name("stable@invalid"),
			Err(Error::InvalidToolchain(msg)) if msg == "Invalid toolchain name: stable@invalid"
		));
		assert!(matches!(
			validate_toolchain_name("nightly/invalid"),
			Err(Error::InvalidToolchain(msg)) if msg == "Invalid toolchain name: nightly/invalid"
		));
		assert!(matches!(
			validate_toolchain_name("stable invalid"),
			Err(Error::InvalidToolchain(msg)) if msg == "Invalid toolchain name: stable invalid"
		));
		assert!(matches!(
			validate_toolchain_name("stable\ninvalid"),
			Err(Error::InvalidToolchain(msg)) if msg == "Invalid toolchain name: stable\ninvalid"
		));
		assert!(matches!(
			validate_toolchain_name(""),
			Err(Error::InvalidToolchain(msg)) if msg == "Invalid toolchain name: "
		));
	}

	#[test]
	fn compare_local_toolchain_matching_succeeds() {
		// Get the current local toolchain
		let rust_toolchain = contract_build::util::rust_toolchain()
			.expect("`rustc` always has a version associated with it.");

		let build_info = BuildInfo {
			rust_toolchain,
			cargo_contract_version: CARGO_CONTRACT_VERSION.clone(),
			build_mode: BuildMode::Release,
		};

		assert!(compare_local_toolchain(&build_info).is_ok());
	}

	#[test]
	fn compare_local_toolchain_mismatched_rust_toolchain() {
		let build_info = BuildInfo {
			rust_toolchain: "nightly-2020-01-01".to_string(),
			cargo_contract_version: CARGO_CONTRACT_VERSION.clone(),
			build_mode: BuildMode::Release,
		};

		let result = compare_local_toolchain(&build_info);
		let expected_msg = "You are trying to `verify` a contract using the following toolchain:\n\
                {rust_toolchain}\n\n\
                However, the original contract was built using this one:\n\
                {expected_rust_toolchain}\n\n\
                Please install the correct toolchain and re-run the `verify` command:\n\
                rustup install {expected_rust_toolchain}";
		assert!(matches!(result, Err(Error::InvalidToolchain(msg)) if msg == expected_msg));
	}

	#[test]
	fn compare_local_toolchain_mismatched_cargo_contract_version() {
		let rust_toolchain = contract_build::util::rust_toolchain()
			.expect("`rustc` always has a version associated with it.");

		let build_info = BuildInfo {
			rust_toolchain,
			cargo_contract_version: semver::Version::parse("1.0.0").unwrap(),
			build_mode: BuildMode::Release,
		};

		let result = compare_local_toolchain(&build_info);
		let expected_msg = format!(
			"\nYou are trying to `verify` a contract using `cargo-contract` version \
                `{}`.\n\n\
                However, the original contract was built using `cargo-contract` version \
                `{}`.\n\n\". The cargo contract version is implied by the `pop` version used.",
			*CARGO_CONTRACT_VERSION, "1.0.0"
		);
		assert!(matches!(result, Err(Error::InvalidToolchain(msg)) if msg == expected_msg));
	}

	#[test]
	fn compare_local_toolchain_invalid_expected_toolchain() {
		let build_info = BuildInfo {
			rust_toolchain: "invalid@toolchain/name".to_string(),
			cargo_contract_version: CARGO_CONTRACT_VERSION.clone(),
			build_mode: BuildMode::Release,
		};

		let result = compare_local_toolchain(&build_info);
		assert!(
			matches!(result, Err(Error::InvalidToolchain(ref msg)) if msg == "Invalid toolchain name: invalid@toolchain/name")
		);
	}

	#[test]
	fn verify_polkavm_blob_against_build_result_succeeds() {
		TestBuilder::default().execute(|builder| {
			// Create a mock polkavm blob
			let polkavm_blob = vec![0x01, 0x02, 0x03, 0x04];
			let blob_hash = CodeHash(contract_build::code_hash(&polkavm_blob))
				.0
				.iter()
				.map(|byte| format!("{:02x}", byte))
				.collect::<String>();

			// Create a mock contract metadata with the same hash
			let metadata = serde_json::json!({
				"source": {
					"hash": blob_hash,
					"language": "ink! 6.0.0",
					"compiler": "rustc 1.0.0",
				},
				"contract":{
					"name": "test",
					"version": "1.0.0",
					"authors": ["test"]
				}
			});

			// Write the metadata to a contract bundle file
			let bundle_path = builder.temp_path().join("test.contract");
			std::fs::write(&bundle_path, metadata.to_string()).unwrap();

			// Create a mock BuildResult
			let artifacts = InkMetadataArtifacts {
				dest_metadata: PathBuf::from("test.wasm"),
				dest_bundle: bundle_path,
			};
			let build_result = BuildResult {
				dest_binary: None,
				metadata_result: Some(MetadataArtifacts::Ink(artifacts)),
				target_directory: PathBuf::new(),
				linker_size_result: None,
				build_mode: BuildMode::Release,
				build_artifact: BuildArtifacts::All,
				verbosity: Verbosity::Default,
				image: None,
				output_type: OutputType::Json,
			};

			let result = verify_polkavm_blob_against_build_result(&polkavm_blob, build_result);
			println!("{:?}", result);
			assert!(result.is_ok());
		});
	}

	#[test]
	fn verify_polkavm_blob_against_build_result_missing_metadata_artifacts() {
		let polkavm_blob = vec![0x01, 0x02, 0x03, 0x04];

		// Create a BuildResult with no metadata_result
		let build_result = BuildResult {
			dest_binary: None,
			metadata_result: None,
			target_directory: PathBuf::new(),
			linker_size_result: None,
			build_mode: BuildMode::Release,
			build_artifact: BuildArtifacts::All,
			verbosity: Verbosity::Default,
			image: None,
			output_type: OutputType::Json,
		};

		let result = verify_polkavm_blob_against_build_result(&polkavm_blob, build_result);
		assert!(matches!(
			result,
			Err(Error::ContractMetadata(msg)) if msg == "The metadata for the workspace contract does not contain a contract binary, therefore we are unable to verify the contract."
		));
	}

	#[test]
	fn verify_polkavm_blob_against_build_result_bundle_file_not_found() {
		let polkavm_blob = vec![0x01, 0x02, 0x03, 0x04];

		// Create a BuildResult pointing to a non-existent bundle file
		let artifacts = InkMetadataArtifacts {
			dest_metadata: PathBuf::from("test.wasm"),
			dest_bundle: PathBuf::from("/nonexistent/bundle.contract"),
		};
		let build_result = BuildResult {
			dest_binary: None,
			metadata_result: Some(MetadataArtifacts::Ink(artifacts)),
			target_directory: PathBuf::new(),
			linker_size_result: None,
			build_mode: BuildMode::Release,
			build_artifact: BuildArtifacts::All,
			verbosity: Verbosity::Default,
			image: None,
			output_type: OutputType::Json,
		};

		let result = verify_polkavm_blob_against_build_result(&polkavm_blob, build_result);
		assert!(matches!(result, Err(Error::IO(err)) if err.kind() == ErrorKind::NotFound));
	}

	#[test]
	fn verify_polkavm_blob_against_build_result_invalid_json_in_bundle() {
		TestBuilder::default().execute(|builder| {
			let polkavm_blob = vec![0x01, 0x02, 0x03, 0x04];

			// Create an invalid JSON file
			let bundle_path = builder.temp_path().join("invalid.contract");
			std::fs::write(&bundle_path, "not valid json").unwrap();

			// Create a BuildResult
			let artifacts = InkMetadataArtifacts {
				dest_metadata: PathBuf::from("test.wasm"),
				dest_bundle: bundle_path,
			};
			let build_result = BuildResult {
				dest_binary: None,
				metadata_result: Some(MetadataArtifacts::Ink(artifacts)),
				target_directory: PathBuf::new(),
				linker_size_result: None,
				build_mode: BuildMode::Release,
				build_artifact: BuildArtifacts::All,
				verbosity: Verbosity::Default,
				image: None,
				output_type: OutputType::Json,
			};

			let result = verify_polkavm_blob_against_build_result(&polkavm_blob, build_result);
			assert!(matches!(result, Err(Error::SerdeJson(msg)) if msg.to_string() == "expected ident at line 1 column 2"));
		});
	}

	#[test]
	fn verify_polkavm_blob_against_build_result_hash_mismatch() {
		TestBuilder::default().execute(|builder| {
			// Create a polkavm blob
			let polkavm_blob = vec![0x01, 0x02, 0x03, 0x04];

            let different_hash = "01".repeat(32);

			let metadata = serde_json::json!({
				"source": {
					"hash": different_hash,
					"language": "ink! 6.0.0",
					"compiler": "rustc 1.0.0",
				},
                "contract":{
                    "name": "test",
                    "version": "1.0.0",
                    "authors": ["test"]
                }
			});

			// Write the metadata to a contract bundle file
			let bundle_path = builder.temp_path().join("test.contract");
			std::fs::write(&bundle_path, metadata.to_string()).unwrap();

			// Create a mock BuildResult
			let artifacts = InkMetadataArtifacts {
				dest_metadata: PathBuf::from("test.wasm"),
				dest_bundle: bundle_path.clone(),
			};

			let build_result = BuildResult {
				dest_binary: None,
				metadata_result: Some(MetadataArtifacts::Ink(artifacts)),
				target_directory: PathBuf::new(),
				linker_size_result: None,
				build_mode: BuildMode::Release,
				build_artifact: BuildArtifacts::All,
				verbosity: Verbosity::Default,
				image: None,
				output_type: OutputType::Json,
			};
			let result = verify_polkavm_blob_against_build_result(&polkavm_blob, build_result);
			assert!(matches!(
				result,
				Err(Error::Verification(msg)) if msg == format!("Failed to verify the polkavm blob against the build result at {:?}.", bundle_path)
			));
		});
	}
}
