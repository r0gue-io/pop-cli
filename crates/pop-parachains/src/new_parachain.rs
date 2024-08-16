// SPDX-License-Identifier: GPL-3.0

use std::{fs, path::Path};

use anyhow::Result;
use pop_common::{
	git::Git,
	templates::{extractor::extract_template_files, Template, Type},
};
use walkdir::WalkDir;

use crate::{
	generator::parachain::{ChainSpec, Network},
	utils::helpers::{sanitize, write_to_file},
	Config, Parachain, Provider,
};

/// Create a new parachain.
///
/// # Arguments
///
/// * `template` - template to generate the parachain from.
/// * `target` - location where the parachain will be created.
/// * `tag_version` - version to use (`None` to use latest).
/// * `config` - customization values to include in the new parachain.
pub fn instantiate_template_dir(
	template: &Parachain,
	target: &Path,
	tag_version: Option<String>,
	config: Config,
) -> Result<Option<String>> {
	sanitize(target)?;

	if Provider::Pop.provides(&template) {
		return instantiate_standard_template(template, target, config, tag_version);
	}
	if Provider::Tanssi.provides(&template) {
		return instantiate_tanssi_template(template, target, tag_version);
	}
	let tag = Git::clone_and_degit(template.repository_url()?, target, tag_version)?;
	Ok(tag)
}

pub fn instantiate_standard_template(
	template: &Parachain,
	target: &Path,
	config: Config,
	tag_version: Option<String>,
) -> Result<Option<String>> {
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	let source = temp_dir.path();

	let tag = Git::clone_and_degit(template.repository_url()?, source, tag_version)?;

	for entry in WalkDir::new(&source) {
		let entry = entry?;

		let source_path = entry.path();
		let destination_path = target.join(source_path.strip_prefix(&source)?);

		if entry.file_type().is_dir() {
			fs::create_dir_all(&destination_path)?;
		} else {
			fs::copy(source_path, &destination_path)?;
		}
	}
	let chainspec = ChainSpec {
		token_symbol: config.symbol,
		decimals: config.decimals,
		initial_endowment: config.initial_endowment,
	};
	use askama::Template;
	write_to_file(
		&target.join("node/src/chain_spec.rs"),
		chainspec.render().expect("infallible").as_ref(),
	)?;
	// Add network configuration
	let network = Network { node: "parachain-template-node".into() };
	write_to_file(&target.join("network.toml"), network.render().expect("infallible").as_ref())?;
	Ok(tag)
}

/// Create a new Tanssi compatible container chain.
///
/// # Arguments
///
/// * `template` - template to generate the parachain from.
/// * `target` - location where the parachain will be created.
/// * `tag_version` - version to use (`None` to use latest).
fn instantiate_tanssi_template(
	template: &Parachain,
	target: &Path,
	tag_version: Option<String>,
) -> Result<Option<String>> {
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	let source = temp_dir.path();
	let tag = Git::clone_and_degit(template.repository_url()?, &source, tag_version)?;

	// Relevant files to extract.
	let files =
		[".rustfmt.toml", "Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "rust-toolchain"];
	for file in files {
		extract_template_files(file.to_string(), temp_dir.path(), target, None)?;
	}

	let owned_target = target.to_owned().to_path_buf();

	// Templates are located in Tanssi's repo as follows:
	// │
	// ├┐ container-chains
	// │├┬ nodes
	// ││└ <template node directories>
	// │└┬ runtime-templates
	// │  └ <tempalte runtime directories>

	// Step 1: extract template node.
	let template_path = format!("container-chains/nodes/{}", template.to_string());
	extract_template_files(
		template_path,
		temp_dir.path(),
		&owned_target.join("node").as_path(),
		None,
	)?;
	// Step 2: extract template runtime.
	let template_path = format!("container-chains/runtime-templates/{}", template.to_string());
	extract_template_files(
		template_path,
		temp_dir.path(),
		&owned_target.join("runtime").as_path(),
		None,
	)?;

	// Add network configuration.
	use askama::Template;
	let network = Network { node: format!("container-chain-{}-node", template.to_string()) };
	write_to_file(&target.join("network.toml"), network.render().expect("infallible").as_ref())?;

	Ok(tag)
}

#[cfg(test)]
mod tests {
	use std::{env::current_dir, fs};

	use anyhow::Result;

	use super::*;

	fn setup_template_and_instantiate() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_standard_template(&Parachain::Standard, temp_dir.path(), config, None)?;
		Ok(temp_dir)
	}

	fn setup_tanssi_template() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		instantiate_tanssi_template(&Parachain::TanssiSimple, temp_dir.path(), None)?;
		Ok(temp_dir)
	}

	#[test]
	fn parachain_instantiate_standard_template_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");

		// Verify that the generated chain_spec.rs file contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("node/src/chain_spec.rs"))
				.expect("Failed to read file");
		assert!(generated_file_content
			.contains("properties.insert(\"tokenSymbol\".into(), \"DOT\".into());"));
		assert!(generated_file_content
			.contains("properties.insert(\"tokenDecimals\".into(), 18.into());"));
		assert!(generated_file_content.contains("1000000"));

		// Verify network.toml contains expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("network.toml")).expect("Failed to read file");
		let expected_file_content =
			fs::read_to_string(current_dir()?.join("./templates/base/network.templ"))
				.expect("Failed to read file");
		assert_eq!(
			generated_file_content,
			expected_file_content.replace("^^node^^", "parachain-template-node")
		);

		Ok(())
	}

	#[test]
	fn instantiate_tanssi_template_works() -> Result<()> {
		let temp_dir = setup_tanssi_template().expect("Failed to instantiate container chain");
		let node_manifest =
			pop_common::manifest::from_path(Some(&temp_dir.path().join("node/Cargo.toml")))
				.expect("Failed to read file");
		assert_eq!("container-chain-simple-node", node_manifest.package().name());

		let node_manifest =
			pop_common::manifest::from_path(Some(&temp_dir.path().join("runtime/Cargo.toml")))
				.expect("Failed to read file");
		assert_eq!("container-chain-template-simple-runtime", node_manifest.package().name());

		Ok(())
	}
}
