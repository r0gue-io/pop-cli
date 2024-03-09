use crate::{
	commands::new::parachain::Template,
	engines::generator::{ChainSpec, Network},
	helpers::{clone_and_degit, sanitize, write_to_file},
};
use anyhow::Result;
use duct::cmd;
use git2::Repository;
use std::{
	fs,
	path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Config {
	pub(crate) symbol: String,
	pub(crate) decimals: u8,
	pub(crate) initial_endowment: String,
}

/// Creates a new template at `target` dir
pub fn instantiate_template_dir(template: &Template, target: &Path, config: Config) -> Result<()> {
	sanitize(target)?;
	use Template::*;
	let url = match template {
		FPT => "https://github.com/paritytech/frontier-parachain-template.git",
		Contracts => "https://github.com/paritytech/substrate-contracts-node.git",
		Base => {
			return instantiate_base_template(target, config);
		},
	};
	clone_and_degit(url, target)?;
	Repository::init(target)?;
	Ok(())
}

pub fn instantiate_base_template(target: &Path, config: Config) -> Result<()> {
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	let source = temp_dir.path();
	clone_and_degit("https://github.com/r0gue-io/base-parachain", source)?;

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
	);
	// Add network configuration
	let network = Network { node: "parachain-template-node".into() };
	write_to_file(&target.join("network.toml"), network.render().expect("infallible").as_ref());
	Repository::init(target)?;
	Ok(())
}

pub fn build_parachain(path: &Option<PathBuf>) -> anyhow::Result<()> {
	cmd("cargo", vec!["build", "--release"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{env::current_dir, fs};
	use tempdir;

	#[test]
	fn test_instantiate_template_dir_base() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempdir::TempDir::new("base_template")?;
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000000000000000000000".to_string(),
		};
		let result: Result<()> = instantiate_base_template(temp_dir.path(), config);
		assert!(result.is_ok());

		// Verify that the generated chain_spec.rs file contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("node/src/chain_spec.rs"))?;
		assert!(generated_file_content.contains("DOT"));
		assert!(generated_file_content.contains("18"));
		assert!(generated_file_content.contains("1000000000000000000000000"));

		// Verify network.toml contains expected content
		let generated_file_content = fs::read_to_string(temp_dir.path().join("network.toml"))?;
		let expected_file_content =
			fs::read_to_string(current_dir()?.join("./templates/base/network.templ"))?;
		assert_eq!(
			generated_file_content,
			expected_file_content.replace("^^node^^", "parachain-template-node")
		);

		Ok(())
	}
}
