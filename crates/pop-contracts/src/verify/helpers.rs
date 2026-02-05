// SPDX-License-Identifier: GPL-3.0

use crate::{
	BuildInfo, BuildMode, CARGO_CONTRACT_VERSION, ContractMetadata, Error, ImageVariant,
	MetadataArtifacts,
};
use contract_build::BuildResult;
use regex::Regex;
use scale::Decode;
use sp_core::{ConstU32, H256, bounded_vec::BoundedVec};
use std::{fs::File, path::Path};
use subxt::ext::scale_encode::EncodeAsType;

#[cfg_attr(test, derive(Debug))]
pub(super) struct BuildInfoParsed {
	pub(super) build_info: BuildInfo,
	pub(super) build_mode: BuildMode,
	pub(super) image: Option<ImageVariant>,
	pub(super) polkavm_code_hash: H256,
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

	// NOTE: pallet_revive stores contracts below its keccack_256 hash, not blake_256 (https://github.com/paritytech/polkadot-sdk/blob/polkadot-unstable2507-revive/substrate/frame/revive/src/vm/pvm/env.rs#L126)
	let polkavm_code_hash = H256::from(sp_core::keccak_256(&polkavm_blob));

	if let Some(info) = metadata.source.build_info {
		let build_info: BuildInfo = serde_json::from_value(info.into())?;
		match metadata.image {
			Some(image) => Ok(BuildInfoParsed {
				build_info,
				build_mode: BuildMode::Verifiable,
				image: Some(ImageVariant::from(Some(image))),
				polkavm_code_hash,
			}),
			_ => {
				let build_mode = build_info.build_mode;
				Ok(BuildInfoParsed { build_info, build_mode, image: None, polkavm_code_hash })
			},
		}
	} else {
		Err(Error::ContractMetadata("The metadata does not contain any build information which can be used to verify a contract.".to_owned()))
	}
}

/// Compare the local toolchain with `BuildInfo`.
///
/// # Arguments
/// - `build_info` - The `BuildInfo` compared to the local toolchain
pub(super) fn compare_local_toolchain(build_info: &BuildInfo) -> Result<(), Error> {
	let expected_rust_toolchain = &build_info.rust_toolchain;
	let rust_toolchain = contract_build::util::rust_toolchain()
		.expect("`rustc` always has a version associated with it.");

	validate_toolchain_name(expected_rust_toolchain)?;

	if rust_toolchain != *expected_rust_toolchain {
		return Err(Error::InvalidToolchain(format!(
			"You are trying to `verify` a contract using the following toolchain:\n\
                {rust_toolchain}\n\n
                However, the original contract was built using:\n\
                {expected_rust_toolchain}\n\n\
                Please install the correct toolchain and re-run the `verify` command:\n\
                rustup install {expected_rust_toolchain}"
		)));
	}

	if build_info.cargo_contract_version != *CARGO_CONTRACT_VERSION {
		return Err(Error::InvalidToolchain(format!(
			"\nYou are trying to verify a contract using `cargo-contract` version \
                `{}`.\n\n
                However, the original contract was built using `cargo-contract` version \
                `{}`.\n\n\". The `cargo contract` version is implied by the `pop` version used.",
			*CARGO_CONTRACT_VERSION, build_info.cargo_contract_version,
		)));
	}

	Ok(())
}

/// Verifies that a PolkaVM code hash matches the contract binary in a build result.
///
/// This function compares this hash against the hash stored in the contract metadata of the build
/// result. This ensures that the contract binary being verified matches what was actually built.
/// NOTE: If this function were public we should take care that the `BuildResult` code hash wasn't
/// manipulated. However it's only available inside this module, and the `BuildResult` that'll be
/// passed will always be legit.
///
/// # Arguments
/// - `polkavm_code_hash` - The PolkaVM codehash to verify
/// - `build_result` - The build result containing the contract metadata to compare against
pub(super) fn verify_polkavm_code_hash_against_build_result(
	polkavm_code_hash: H256,
	build_result: BuildResult,
) -> Result<(), Error> {
	let built_contract_path = if let Some(MetadataArtifacts::Ink(artifacts)) =
		build_result.metadata_result
	{
		artifacts
	} else {
		return Err(Error::ContractMetadata("The metadata for the workspace contract does not contain a contract binary, therefore we are unable to verify the contract.".to_owned()));
	};

	let file = File::open(&built_contract_path.dest_bundle)?;
	let built_contract: ContractMetadata = serde_json::from_reader(file)?;

	if polkavm_code_hash.0 != built_contract.source.hash.0 {
		return Err(Error::Verification(
			"The verification failed. The two contracts don't produce the same bytecode."
				.to_owned(),
		));
	}

	Ok(())
}

// NOTE: This struct is needed to decode the contract info from pallet_revive storage.  So this may be changing regularly as pallet_revive isn't precisely stable (tho this struct may not change a lot). The struct has been taken from https://github.com/paritytech/polkadot-sdk/blob/polkadot-unstable2507-revive/substrate/frame/revive/src/storage.rs#L71 which is the latest supported version. To allow compatibility with all chains using this version of revive, we remove the generic (depending on the pallet), and do the (fair) assumption that all balances are `u128`, as most parachains use this type in their config: https://paritytech.github.io/polkadot-sdk/master/parachains_common/type.Balance.html. That way we remove the generics depending on the runtime
#[derive(Decode)]
struct AccountInfo {
	// The type of the account.
	account_type: AccountType,
	// The  amount that was transferred to this account that is less than the
	// NativeToEthRatio, and can be represented in the native currency
	#[allow(dead_code)]
	dust: u32,
}

#[derive(Decode)]
enum AccountType {
	Contract(ContractInfo),
	Eoa,
}

type TrieId = BoundedVec<u8, ConstU32<128>>;

#[derive(Decode)]
struct ContractInfo {
	// Unique ID for the subtree encoded as a bytes vector.
	#[allow(dead_code)]
	trie_id: TrieId,
	// The code associated with a given account.
	code_hash: H256,
	// How many bytes of storage are accumulated in this contract's child trie.
	#[allow(dead_code)]
	storage_bytes: u32,
	// How many items of storage are accumulated in this contract's child trie.
	#[allow(dead_code)]
	storage_items: u32,
	// This records to how much deposit the accumulated `storage_bytes` amount to.
	#[allow(dead_code)]
	storage_byte_deposit: u128,
	// This records to how much deposit the accumulated `storage_items` amount to.
	#[allow(dead_code)]
	storage_item_deposit: u128,
	// This records how much deposit is put down in order to pay for the contract itself.
	//
	// We need to store this information separately so it is not used when calculating any refunds
	// since the base deposit can only ever be refunded on contract termination.
	#[allow(dead_code)]
	pub storage_base_deposit: u128,
	#[allow(dead_code)]
	// The size of the immutable data of this contract.
	pub immutable_data_len: u32,
}

pub(super) async fn get_deployed_polkavm_code_hash(
	rpc_endpoint: &str,
	contract_address: &str,
) -> Result<H256, Error> {
	let contract_address = crate::utils::parse_hex_bytes(contract_address)?.0;
	let client = pop_chains::set_up_client(rpc_endpoint).await.map_err(|_| {
		Error::Verification(
			"pop couldn't connect to the provided rpc endpoint. Verification aborted.".to_owned(),
		)
	})?;

	let account_info_of_storage = pop_chains::parse_chain_metadata(&client)
        .map_err(|_|Error::Verification("`pop` couldn't parse metadata from the provided endpoint. Verification aborted.".to_owned()))?
		.into_iter()
		.find(|pallet| pallet.name == "Revive")
		.ok_or(Error::Verification(
			"The target chain doesn't support smart contracts. Verification aborted.".to_owned(),
		))?
        .state
        .into_iter()
        .find(|storage| storage.name == "AccountInfoOf")
		.ok_or(Error::Verification("revive.AccountInfoOf not found in the specified chain. This chain isn't using the latest revive version and hence isn't supported by pop".to_owned()))?;

	let storage_value = account_info_of_storage
		.query(&client, vec![contract_address.into()])
		.await
        .map_err(|_|Error::Verification("`pop` cannot find contract information for the provided contract address. Verification aborted.".to_owned()))?
		.and_then(|storage_value| {
			storage_value.encode_as_type(account_info_of_storage.type_id, client.metadata().types()).ok()
		})
		.ok_or(Error::Verification("`pop` cannot find contract information for the provided contract address. Verification aborted.".to_owned()))?;

	let account_info = AccountInfo::decode(&mut &storage_value[..]).map_err(|_| Error::Verification("`pop` cannot find contract information for the provided contract address. Verification aborted.".to_owned()))?;
	let code_hash = match account_info.account_type {
		AccountType::Contract(ContractInfo { code_hash, .. }) => code_hash,
		_ => {
			return Err(Error::Verification(
				"The provided address doesn't belong to a contract. Verification aborted."
					.to_owned(),
			));
		},
	};

	Ok(code_hash)
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
		BuildArtifacts, OutputType, Verbosity,
		metadata::{InkMetadataArtifacts, MetadataArtifacts},
	};
	use pop_common::test_env::TestNode;
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
				assert_eq!(
					format!("{:?}", result.image.unwrap()),
					"Custom(\"useink/contracts-verifiable:6.0.0-beta.1\")"
				);
				assert_eq!(
					result.polkavm_code_hash.0,
					[
						192, 235, 212, 95, 142, 195, 172, 151, 29, 73, 74, 170, 83, 227, 95, 172,
						94, 254, 37, 225, 134, 215, 167, 254, 224, 101, 10, 229, 232, 96, 121, 40
					]
				);
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
				assert_eq!(
					result.polkavm_code_hash.0,
					[
						251, 6, 126, 132, 31, 3, 8, 176, 14, 18, 248, 17, 119, 139, 46, 26, 198,
						234, 173, 216, 243, 192, 30, 144, 122, 71, 228, 29, 1, 143, 244, 189
					]
				);
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
		let expected_rust_toolchain = "nightly-2020-01-01";
		let rust_toolchain = contract_build::util::rust_toolchain()
			.expect("`rustc` always has a version associated with it.");

		let result = compare_local_toolchain(&build_info);
		let expected_msg = format!(
			"You are trying to `verify` a contract using the following toolchain:\n\
                {rust_toolchain}\n\n
                However, the original contract was built using:\n\
                {expected_rust_toolchain}\n\n\
                Please install the correct toolchain and re-run the `verify` command:\n\
                rustup install {expected_rust_toolchain}"
		);

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
			"\nYou are trying to verify a contract using `cargo-contract` version \
                `{}`.\n\n
                However, the original contract was built using `cargo-contract` version \
                `{}`.\n\n\". The `cargo contract` version is implied by the `pop` version used.",
			*CARGO_CONTRACT_VERSION, "1.0.0"
		);
		println!("{:?}", result);
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
	fn verify_polkavm_code_hash_against_build_result_succeeds() {
		TestBuilder::default().execute(|builder| {
			// Create a mock polkavm blob
			let polkavm_blob = vec![0x01, 0x02, 0x03, 0x04];
			let polkavm_code_hash = H256::from(sp_core::keccak_256(&polkavm_blob));

			// Create a mock contract metadata with the same hash
			let metadata = serde_json::json!({
				"source": {
					"hash": polkavm_code_hash,
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

			let result =
				verify_polkavm_code_hash_against_build_result(polkavm_code_hash, build_result);
			assert!(result.is_ok());
		});
	}

	#[test]
	fn verify_polkavm_code_hash_against_build_result_missing_metadata_artifacts() {
		let polkavm_code_hash = H256::from([1; 32]);

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

		let result = verify_polkavm_code_hash_against_build_result(polkavm_code_hash, build_result);
		assert!(matches!(
			result,
			Err(Error::ContractMetadata(msg)) if msg == "The metadata for the workspace contract does not contain a contract binary, therefore we are unable to verify the contract."
		));
	}

	#[test]
	fn verify_polkavm_code_hash_against_build_result_bundle_file_not_found() {
		let polkavm_code_hash = H256::from([1; 32]);

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

		let result = verify_polkavm_code_hash_against_build_result(polkavm_code_hash, build_result);
		assert!(matches!(result, Err(Error::IO(err)) if err.kind() == ErrorKind::NotFound));
	}

	#[test]
	fn verify_polkavm_code_hash_against_build_result_invalid_json_in_bundle() {
		TestBuilder::default().execute(|builder| {
		let polkavm_code_hash = H256::from([1; 32]);

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

			let result = verify_polkavm_code_hash_against_build_result(polkavm_code_hash, build_result);
			assert!(matches!(result, Err(Error::SerdeJson(msg)) if msg.to_string() == "expected ident at line 1 column 2"));
		});
	}

	#[test]
	fn verify_polkavm_code_hash_against_build_result_hash_mismatch() {
		TestBuilder::default().execute(|builder| {
		let polkavm_code_hash = H256::from([1; 32]);

            let different_hash = "02".repeat(32);

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
			let result = verify_polkavm_code_hash_against_build_result(polkavm_code_hash, build_result);
			assert!(matches!(
				result,
				Err(Error::Verification(msg)) if msg == "The verification failed. The two contracts don't produce the same bytecode."
			));
		});
	}

	// Tests for get_deployed_polkavm_code_hash

	#[tokio::test]
	async fn get_deployed_polkavm_code_hash_fails_with_invalid_address_format() {
		// Test with invalid hex address - this fails during address parsing before connecting
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let result = get_deployed_polkavm_code_hash(node.ws_url(), "invalid_address").await;

		// Should fail during parse_hex_bytes
		assert!(matches!(result, Err(Error::HexParsing(msg)) if msg == "Odd number of digits"));
	}

	#[tokio::test]
	async fn get_deployed_polkavm_code_hash_fails_with_invalid_rpc_endpoint() {
		// Test with completely invalid URL
		let result = get_deployed_polkavm_code_hash(
			"wss://nonexistent.invalid.endpoint.test",
			"0x0000000000000000000000000000000000000000",
		)
		.await;

		assert!(matches!(
			result,
			Err(Error::Verification(msg)) if msg == "pop couldn't connect to the provided rpc endpoint. Verification aborted."
		));
	}

	#[tokio::test]
	async fn get_deployed_polkavm_code_hash_fails_with_nonexistent_contract_address() {
		// Use a valid address format that doesn't exist on chain
		let nonexistent_address = "0x0000000000000000000000000000000000000000";

		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let result = get_deployed_polkavm_code_hash(node.ws_url(), nonexistent_address).await;

		assert!(matches!(
			result,
			Err(Error::Verification(msg)) if msg == "`pop` cannot find contract information for the provided contract address. Verification aborted."
		));
	}
}
