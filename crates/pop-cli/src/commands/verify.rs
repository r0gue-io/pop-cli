// SPDX-License-Identifier: GPL-3.0

use crate::{cli::traits::Cli, common::builds::PopComposeBuildArgs};
use anyhow::{Context, Result};
use clap::Args;
use pop_contracts::{
	BuildInfo, BuildMode, CARGO_CONTRACT_VERSION, CodeHash, ContractMetadata, ImageVariant,
	MetadataArtifacts, Verbosity,
};
use regex::Regex;
use serde::Serialize;
use std::{fs::File, path::PathBuf};

#[derive(Args, Serialize)]
pub(crate) struct VerifyCommand {
	/// Directory path with flag for your project [default: current directory]
	#[clap(short, long)]
	path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The reference `.contract` file (`*.contract`) that the selected
	/// contract will be checked against.
	#[clap(short, long)]
	contract_path: PathBuf,
}

impl VerifyCommand {
	pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Start verifying your contract, this make take a bit ⏳")?;

		let project_path =
			crate::common::builds::ensure_project_path(self.path.clone(), self.path_pos.clone());

		self.verify_contract(project_path)?;

		Ok(())
	}

	fn verify_contract(&self, project_path: PathBuf) -> Result<()> {
		// 1. Read the given metadata, and pull out the `BuildInfo`
		let file = File::open(&self.contract_path)
			.context(format!("Failed to open contract bundle {}", self.contract_path.display()))?;

		let metadata: ContractMetadata = serde_json::from_reader(&file).context(format!(
			"Failed to deserialize contract bundle {}",
			self.contract_path.display()
		))?;
		let build_info = if let Some(info) = metadata.source.build_info {
			info
		} else {
			anyhow::bail!(
				"\nThe metadata does not contain any build information which can be used to \
                verify a contract."
					.to_string()
			)
		};

		let build_info: BuildInfo = serde_json::from_value(build_info.into()).context(format!(
			"Failed to deserialize the build info from {}",
			self.contract_path.display()
		))?;

		let build_mode = if metadata.image.is_some() {
			// TODO: Ensure Docker is running or stop the build
			BuildMode::Verifiable
		} else {
			build_info.build_mode
		};

		// 2. Check that the build info from the metadata matches our current setup.
		// if the build mode isn't verifiable
		if build_mode != BuildMode::Verifiable {
			let expected_rust_toolchain = build_info.rust_toolchain;
			let rust_toolchain = pop_contracts::rust_toolchain()
				.expect("`rustc` always has a version associated with it.");

			validate_toolchain_name(&expected_rust_toolchain)?;
			validate_toolchain_name(&rust_toolchain)?;

			let mismatched_rustc = format!(
				"\nYou are trying to `verify` a contract using the following toolchain:\n\
                {rust_toolchain}\n\n\
                However, the original contract was built using this one:\n\
                {expected_rust_toolchain}\n\n\
                Please install the correct toolchain and re-run the `verify` command:\n\
                rustup install {expected_rust_toolchain}"
			);
			anyhow::ensure!(rust_toolchain == expected_rust_toolchain, mismatched_rustc);

			let expected_cargo_contract_version = build_info.cargo_contract_version;

			let mismatched_cargo_contract = format!(
				"\nYou are trying to `verify` a contract using `cargo-contract` version \
                `{}`.\n\n\
                However, the original contract was built using `cargo-contract` version \
                `{expected_cargo_contract_version}`.\n\n\
                Please install the matching version and re-run the `verify` command:\n\
                cargo install --force --locked cargo-contract --version {expected_cargo_contract_version}",
				*CARGO_CONTRACT_VERSION
			);
			anyhow::ensure!(
				expected_cargo_contract_version == *CARGO_CONTRACT_VERSION,
				mismatched_cargo_contract
			);
		}

		// 3a. Build contract with the `BuildInfo` from the metadata.
		let build_result = pop_contracts::build_smart_contract::<PopComposeBuildArgs>(
			&project_path,
			build_mode,
			Verbosity::default(),
			None,
			Some(ImageVariant::from(metadata.image.clone())),
		)?;

		// 4. Grab the code hash from the built contract and compare it with the reference code
		//    hash.
		//
		//    We compute the hash of the reference code here, instead of relying on
		//    the `source.hash` field in the metadata. This is because the `source.hash`
		//    field could have been manipulated; we want to be sure that _the code_ of
		//    both contracts is equal.
		let reference_polkavm_blob = pop_contracts::decode_hex(
			&metadata
				.source
				.contract_binary
				.expect("no `source.polkavm` field exists in metadata")
				.to_string(),
		)
		.expect("decoding the `source.polkavm` hex failed");
		let reference_code_hash = CodeHash(pop_contracts::code_hash(&reference_polkavm_blob));
		let built_contract_path =
			if let Some(MetadataArtifacts::Ink(m)) = build_result.metadata_result {
				m
			} else {
				// Since we're building the contract ourselves this should always be
				// populated, but we'll bail out here just in case.
				anyhow::bail!(
                "\nThe metadata for the workspace contract does not contain a contract binary,\n\
                therefore we are unable to verify the contract."
                .to_string()
            )
			};

		let target_bundle = &built_contract_path.dest_bundle;

		let file = File::open(target_bundle.clone())
			.context(format!("Failed to open contract bundle {}", target_bundle.display()))?;
		let built_contract: ContractMetadata = serde_json::from_reader(file).context(format!(
			"Failed to deserialize contract bundle {}",
			target_bundle.display()
		))?;

		let target_code_hash = built_contract.source.hash;

		if reference_code_hash != target_code_hash {
			anyhow::bail!(format!(
				"\nFailed to verify `{}` against the workspace at `{}`: the hashed polkavm blobs are not matching.",
				format!("{}", self.contract_path.display()),
				format!("{}", project_path.display())
			));
		}

		// check that the metadata hash is the same as reference_code_hash
		if reference_code_hash != metadata.source.hash {
			anyhow::bail!(format!(
				"\nThe reference contract `{}` metadata is corrupt: the `source.hash` does not match the `source.polkavm` hash.",
				format!("{}", self.contract_path.display())
			));
		}

		let verification = VerificationResult {
			is_verified: true,
			image: metadata.image,
			contract: target_bundle.display().to_string(),
			reference_contract: self.contract_path.display().to_string(),
			verbosity: Verbosity::default(),
		};

		println!("{}", verification.serialize_json()?);

		Ok(())
	}
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
fn validate_toolchain_name(toolchain: &str) -> Result<()> {
	let re = Regex::new(r"^[a-zA-Z._\-0-9]+$").expect("failed creating regex");
	if re.is_match(toolchain) {
		return Ok(());
	}
	anyhow::bail!("Invalid toolchain name: {toolchain}")
}

/// The result of verification process
#[derive(serde::Serialize, serde::Deserialize)]
pub struct VerificationResult {
	pub is_verified: bool,
	pub image: Option<String>,
	pub contract: String,
	pub reference_contract: String,
	#[serde(skip_serializing, skip_deserializing)]
	pub verbosity: Verbosity,
}

impl VerificationResult {
	/// Display the build results in a pretty formatted JSON string.
	pub fn serialize_json(&self) -> Result<String> {
		Ok(serde_json::to_string_pretty(self)?)
	}
}
