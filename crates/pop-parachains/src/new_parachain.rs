// SPDX-License-Identifier: GPL-3.0

use crate::{utils::helpers::sanitize, Config, Provider, Template};
use anyhow::Result;
use cargo_generate::{GenerateArgs, TemplatePath};
use pop_common::git::Git;
use std::{fs, path::Path};

/// Create a new parachain.
///
/// # Arguments
///
/// * `template` - template to generate the parachain from.
/// * `target` - location where the parachain will be created.
/// * `tag_version` - version to use (`None` to use latest).
/// * `config` - customization values to include in the new parachain.
pub fn instantiate_template_dir(
	template: &Template,
	target: &Path,
	tag_version: Option<String>,
	config: Config,
) -> Result<Option<String>> {
	sanitize(target)?;

	if template.matches(&Provider::Pop) {
		return instantiate_standard_template(template, target, config, tag_version);
	}
	let tag = Git::clone_and_degit(template.repository_url()?, target, tag_version)?;
	Ok(tag)
}

pub fn instantiate_standard_template(
	template: &Template,
	target: &Path,
	config: Config,
	tag_version: Option<String>,
) -> Result<Option<String>> {
	// Template palceholder definitions
	let mut token_symbol: String = "token-symbol=".to_string();
	let mut token_decimals: String = "token-decimals=".to_string();
	let mut initial_endowement: String = "initial-endowment=".to_string();

	// Placeholder customization
	token_symbol.push_str(&*config.symbol);
	token_decimals.push_str(&*config.decimals);
	initial_endowement.push_str(&*config.initial_endowment);

	// Tempalte rendering arguments
	let standard_template_path = TemplatePath {
		git: Some(
			template
				.repository_url()
				.map_or(String::from("https://github.com/r0gue.io/base-parachain"), |r| {
					r.to_string()
				}),
		),
		tag: tag_version.clone(),
		..Default::default()
	};

	let template_generation_args = GenerateArgs {
		template_path: standard_template_path,
		name: Some(target.file_name().unwrap().to_str().unwrap().to_string()),
		destination: Some(target.parent().unwrap().to_path_buf()),
		define: vec![token_symbol, token_decimals, initial_endowement],
		..Default::default()
	};

	// Template rendering
	let target_template_path = cargo_generate::generate(template_generation_args)
		.expect("Couldn't render liquid tempalte");

	// Degit
	let target_dirs = vec![".git", ".github"];
	let remove_target = Path::new(&target_template_path);
	for dir in target_dirs {
		let git_dir = Path::new(dir);
		fs::remove_dir_all(&remove_target.join(git_dir))?;
	}

	Ok(tag_version)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::{env::current_dir, fs};

	fn setup_template_and_instantiate() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		println!("{:?}", temp_dir);
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: "18".to_string(),
			initial_endowment: "1000000".to_string(),
		};
		let _ = sanitize(temp_dir.path());
		instantiate_standard_template(
			&Template::Standard,
			temp_dir.path(),
			config,
			Some(String::from("liquid-template")),
		)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_parachain_instantiate_standard_template() -> Result<()> {
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
		let mut expected_file_content =
			temp_dir.path().file_name().unwrap().to_str().unwrap().to_string().to_owned();
		expected_file_content.push_str("-node");
		assert!(generated_file_content.contains(&expected_file_content));

		Ok(())
	}
}
