// SPDX-License-Identifier: GPL-3.0

use crate::{
	ChainTemplate, Config, Provider,
	generator::chain::{ChainSpec, Network},
	utils::helpers::{sanitize, write_to_file},
};
use anyhow::Result;
use pop_common::{
	git::Git,
	templates::{Template, Type, extractor::extract_template_files},
};
use std::{fs, path::Path};
use walkdir::WalkDir;

// The latest version where we should overwrite the ./network.toml file on.
const LATEST_NETWORK_TOML_OVERWRITE_VERSION: &str = "polkadot-stable2506";

/// Create a new chain.
///
/// # Arguments
///
/// * `template` - template to generate the chain from.
/// * `target` - location where the chain will be created.
/// * `tag_version` - version to use (`None` to use latest).
/// * `config` - customization values to include in the new chain.
pub fn instantiate_template_dir(
	template: &ChainTemplate,
	target: &Path,
	tag_version: Option<String>,
	config: Config,
) -> Result<Option<String>> {
	sanitize(target)?;

	if Provider::Pop.provides(template) || template == &ChainTemplate::ParityGeneric {
		return instantiate_standard_template(template, target, config, tag_version);
	}
	if Provider::OpenZeppelin.provides(template) {
		return instantiate_openzeppelin_template(template, target, tag_version);
	}
	let tag = Git::clone_and_degit(template.repository_url()?, target, tag_version)?;
	Ok(tag)
}

pub fn instantiate_standard_template(
	template: &ChainTemplate,
	target: &Path,
	config: Config,
	tag_version: Option<String>,
) -> Result<Option<String>> {
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	let source = temp_dir.path();

	let tag = Git::clone_and_degit(template.repository_url()?, source, tag_version.clone())?;

	for entry in WalkDir::new(source) {
		let entry = entry?;

		let source_path = entry.path();
		let destination_path = target.join(source_path.strip_prefix(source)?);

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
		based_on: template.to_string(),
	};
	if let Some(version) = tag_version.as_deref() &&
		version <= LATEST_NETWORK_TOML_OVERWRITE_VERSION
	{
		use askama::Template;
		let chain_spec_path = target.join("node/src/chain_spec.rs");
		if chain_spec_path.parent().is_some_and(|p| p.exists()) {
			write_to_file(&chain_spec_path, chainspec.render().expect("infallible").as_ref())?;
		}
		// Add network configuration
		let network = Network { node: "parachain-template-node".into() };
		write_to_file(
			&target.join("network.toml"),
			network.render().expect("infallible").as_ref(),
		)?;
	}
	Ok(tag)
}

pub fn instantiate_openzeppelin_template(
	template: &ChainTemplate,
	target: &Path,
	tag_version: Option<String>,
) -> Result<Option<String>> {
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	let source = temp_dir.path();

	let tag = Git::clone_and_degit(template.repository_url()?, source, tag_version)?;
	let template_name = template.template_name_without_provider();
	extract_template_files(template_name, temp_dir.path(), target, None)?;
	Ok(tag)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::{env::current_dir, fs};

	fn setup_template_and_instantiate() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_standard_template(
			&ChainTemplate::Standard,
			temp_dir.path(),
			config,
			Some("polkadot-stable2506".to_string()),
		)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_chain_instantiate_standard_template() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");

		// Verify that the generated chain_spec.rs file contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("node/src/chain_spec.rs"))
				.expect("Failed to read file");
		assert!(
			generated_file_content
				.contains("properties.insert(\"tokenSymbol\".into(), \"DOT\".into());")
		);
		assert!(
			generated_file_content
				.contains("properties.insert(\"tokenDecimals\".into(), 18.into());")
		);
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
	fn test_chain_instantiate_openzeppelin_template() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		instantiate_openzeppelin_template(&ChainTemplate::OpenZeppelinEVM, temp_dir.path(), None)?;

		let node_manifest =
			pop_common::manifest::from_path(&temp_dir.path().join("node/Cargo.toml"))
				.expect("Failed to read file");
		assert_eq!("evm-template-node", node_manifest.package().name());

		let runtime_manifest =
			pop_common::manifest::from_path(&temp_dir.path().join("runtime/Cargo.toml"))
				.expect("Failed to read file");
		assert_eq!("evm-runtime-template", runtime_manifest.package().name());

		Ok(())
	}
}
