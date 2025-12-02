// SPDX-License-Identifier: GPL-3.0

use pop_contracts::{CodeHash, ContractMetadata, ManifestPath, BuildInfo, BuildMode};
use crate::cli::traits::Cli;
use anyhow::Result;
use clap::Args;
use serde::Serialize;
use std::{
	path::PathBuf,
	process::Command as StdCommand,
    fs::File
};
use semver::Version;
use regex::Regex;


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

impl VerifyCommand{
    pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<()>{
        cli.intro("Start verifying your contract, this make take a bit ⏳")?;

        let project_path =
			crate::common::builds::ensure_project_path(self.path.clone(), self.path_pos.clone());

        let manifest_path = pop_contracts::utils::get_manifest_path(project_path)?;

        self.verify_contract(manifest_path)?;

        Ok(())
    }

    fn verify_contract(&self, manifest_path: ManifestPath) -> Result<()>{
        // 1. Read the given metadata, and pull out the `BuildInfo`
        let file = File::open(self.contract_path)
            .context(format!("Failed to open contract bundle {}", self.contract_path.display()))?;

        let metadata: ContractMetadata = serde_json::from_reader(&file).context(
            format!("Failed to deserialize contract bundle {}", self.contract_path.display()),
        )?;
        let build_info = if let Some(info) = metadata.source.build_info {
            info
        } else {
            anyhow::bail!(
                "\nThe metadata does not contain any build information which can be used to \
                verify a contract."
                .to_string()
            )
        };

        let build_info: BuildInfo =
            serde_json::from_value(build_info.into()).context(format!(
                "Failed to deserialize the build info from {}",
                self.path.display()
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
            let cargo_contract_version = semver::Version::parse(VERSION)?;

            let mismatched_cargo_contract = format!(
                "\nYou are trying to `verify` a contract using `cargo-contract` version \
                `{cargo_contract_version}`.\n\n\
                However, the original contract was built using `cargo-contract` version \
                `{expected_cargo_contract_version}`.\n\n\
                Please install the matching version and re-run the `verify` command:\n\
                cargo install --force --locked cargo-contract --version {expected_cargo_contract_version}",
            );
            anyhow::ensure!(
                cargo_contract_matches,
                mismatched_cargo_contract.bright_yellow()
            );
        }

        // 3a. Call `cargo contract build` with the `BuildInfo` from the metadata.
        let args = ExecuteArgs {
            manifest_path: manifest_path.clone(),
            verbosity,
            build_mode,
            build_artifact: BuildArtifacts::All,
            image: ImageVariant::from(metadata.image.clone()),
            extra_lints: false,
            ..Default::default()
        };

        let build_result = execute(args)?;

        // 4. Grab the code hash from the built contract and compare it with the reference
        //    code hash.
        //
        //    We compute the hash of the reference code here, instead of relying on
        //    the `source.hash` field in the metadata. This is because the `source.hash`
        //    field could have been manipulated; we want to be sure that _the code_ of
        //    both contracts is equal.
        let reference_polkavm_blob = decode_hex(
            &metadata
                .source
                .contract_binary
                .expect("no `source.polkavm` field exists in metadata")
                .to_string(),
        )
        .expect("decoding the `source.polkavm` hex failed");
        let reference_code_hash = CodeHash(code_hash(&reference_polkavm_blob));
        let built_contract_path = if let Some(MetadataArtifacts::Ink(m)) =
            build_result.metadata_result
        {
            m
        } else {
            // Since we're building the contract ourselves this should always be
            // populated, but we'll bail out here just in case.
            anyhow::bail!(
                "\nThe metadata for the workspace contract does not contain a contract binary,\n\
                therefore we are unable to verify the contract."
                .to_string()
                .bright_yellow()
            )
        };

        let target_bundle = &built_contract_path.dest_bundle;

        let file = File::open(target_bundle.clone()).context(format!(
            "Failed to open contract bundle {}",
            target_bundle.display()
        ))?;
        let built_contract: ContractMetadata =
            serde_json::from_reader(file).context(format!(
                "Failed to deserialize contract bundle {}",
                target_bundle.display()
            ))?;

        let target_code_hash = built_contract.source.hash;

        if reference_code_hash != target_code_hash {
            verbose_eprintln!(
                verbosity,
                "Expected code hash from reference contract ({}): {}\nGot Code Hash: {}\n",
                &path.display(),
                &reference_code_hash,
                &target_code_hash
            );
            anyhow::bail!(format!(
                "\nFailed to verify `{}` against the workspace at `{}`: the hashed polkavm blobs are not matching.",
                format!("{}", &path.display()).bright_white(),
                format!("{}", manifest_path.as_ref().display()).bright_white()
            )
            .bright_red());
        }

        // check that the metadata hash is the same as reference_code_hash
        if reference_code_hash != metadata.source.hash {
            verbose_eprintln!(
                verbosity,
                "Expected code hash from reference metadata ({}): {}\nGot Code Hash: {}\n",
                &path.display(),
                &reference_code_hash,
                &metadata.source.hash
            );
            anyhow::bail!(format!(
                "\nThe reference contract `{}` metadata is corrupt: the `source.hash` does not match the `source.polkavm` hash.",
                format!("{}", &path.display()).bright_white()
            )
            .bright_red());
        }

        Ok(VerificationResult {
            is_verified: true,
            image: metadata.image,
            contract: target_bundle.display().to_string(),
            reference_contract: path.display().to_string(),
            output_json: self.output_json,
            verbosity,
        })

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