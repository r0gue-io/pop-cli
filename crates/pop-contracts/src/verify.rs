// SPDX-License-Identifier: GPL-3.0

mod helpers;

use crate::{
	BuildInfo, BuildMode, CARGO_CONTRACT_VERSION, CodeHash, ContractMetadata, Error, ImageVariant,
	MetadataArtifacts, Verbosity,
};
use contract_build::{BuildResult, ComposeBuildArgs};
use core::marker::PhantomData;
use regex::Regex;
use serde::Serialize;
use std::{fs::File, path::PathBuf};

/// A struct representing a contract deployed on-chain.
pub struct DeployedContract {
	/// An endpoint to the chain where the contract is deployed.
	pub rpc_endpoint: String,
	/// The contract address.
	pub contract_address: String,
	/// The image used to build the deployed contract.
	pub build_image: ImageVariant,
}

/// The reference contract, either local or deployed on chain.
enum ReferenceContract {
	Local(PathBuf),
	Deployed(DeployedContract),
}

/// A struct used to verify a contract
pub struct VerifyContract<T: ComposeBuildArgs> {
	/// The path containing the source contract to be verified against `reference_contract`.
	verifying_path: PathBuf,
	/// The reference contract to verify against
	reference_contract: ReferenceContract,
	_marker: PhantomData<T>,
}

impl<T: ComposeBuildArgs> VerifyContract<T> {
	/// Creates a new `VerifyContract` instance used to verify against a local bundle.
	///
	/// #Arguments
	/// - `verifying_path` - The path to the local project being verified.
	/// - `reference_contract_bundle_path` - The path to the ".contract" bundle to verify against.
	pub fn new_local(verifying_path: PathBuf, reference_contract_bundle_path: PathBuf) -> Self {
		Self {
			verifying_path,
			reference_contract: ReferenceContract::Local(reference_contract_bundle_path),
			_marker: PhantomData,
		}
	}

	/// Creates a new `VerifyContract` instance used to verify against a deployed contract.
	///
	/// #Arguments
	/// - `verifying_path` - The path to the local project being verified.
	/// - `reference_deployed_contract` - The deployed contract info.
	pub fn new_deployed(
		verifying_path: PathBuf,
		reference_deployed_contract: DeployedContract,
	) -> Self {
		Self {
			verifying_path,
			reference_contract: ReferenceContract::Deployed(reference_deployed_contract),
			_marker: PhantomData,
		}
	}

	/// Verify the contract
	pub fn execute(self) -> Result<(), Error> {
		match self.reference_contract {
			ReferenceContract::Local(reference_path) => {
				// Parse the contract bundle
				let build_info_parsed =
					helpers::get_build_info_parsed_from_contract_bundle(&reference_path)?;

				// If reference was built in a verifiable mode, just ensure Docker is running so we
				// can run the image. Otherwise check that the local toolchain is the same one used
				// to compile the reference
				if let BuildMode::Verifiable = &build_info_parsed.build_mode {
					// TODO: Ensure Docker is running (https://github.com/r0gue-io/pop-cli/pull/811 needs to be merged)
					()
				} else {
					helpers::compare_local_toolchain(&build_info_parsed.build_info)?;
				}

				let build_result = crate::build_smart_contract::<T>(
					&self.verifying_path,
					build_info_parsed.build_mode,
					Verbosity::default(),
					None,
					build_info_parsed.image,
				)?;

				helpers::verify_polkavm_blob_against_build_result(
					&build_info_parsed.polkavm_blob,
					build_result,
				)?;
				Ok(())
			},
			ReferenceContract::Deployed(_) => Ok(()),
		}
	}
}
