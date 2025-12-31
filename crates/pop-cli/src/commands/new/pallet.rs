// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	common::helpers::check_destination_path,
	multiselect_pick,
};

use clap::{Args, Subcommand};
use cliclack::multiselect;
use pop_chains::{
	TemplatePalletConfig, TemplatePalletConfigCommonTypes, TemplatePalletOptions,
	TemplatePalletStorageTypes, create_pallet_template,
};
use serde::Serialize;
use std::{path::PathBuf, process::Command};
use strum::{EnumMessage, IntoEnumIterator};

fn after_help_simple() -> &'static str {
	r#"Examples:
        pop new pallet
            -> Will create a simple pallet, you'll have to choose your pallet name.
        pop new pallet my-pallet
            -> Will automatically create a pallet called my-pallet in the current directory.
        pop new pallet pallets/my-pallet
            -> Will automatically create a pallet called my pallet in the directory ./pallets
        pop new pallet advanced
            -> Will unlock the advanced mode. pop new pallet advanced --help for further info.
    "#
}

fn after_help_advanced() -> &'static str {
	r#"
        Examples:
            pop new pallet my-pallet advanced
                -> If no [OPTIONS] are specified, the interactive advanced mode is launched.
            pop new pallet my-pallet advanced --config-common-types runtime-origin currency --storage storage-value storage-map -d
                -> Using some [OPTIONS] will execute the non-interactive advanced mode.
    "#
}

#[derive(Args, Serialize)]
#[cfg_attr(test, derive(Default))]
#[command(after_help= after_help_simple())]
pub struct NewPalletCommand {
	#[arg(help = "Name of the pallet")]
	pub(crate) name: Option<String>,
	#[arg(short, long, help = "Name of authors", default_value = "Anonymous")]
	pub(crate) authors: Option<String>,
	#[arg(short, long, help = "Pallet description", default_value = "Frame Pallet")]
	pub(crate) description: Option<String>,
	#[command(subcommand)]
	pub(crate) mode: Option<Mode>,
}

#[derive(Subcommand, Serialize)]
pub enum Mode {
	/// Advanced mode enables more detailed customization of pallet development.
	Advanced(AdvancedMode),
}

#[derive(Args, Serialize)]
#[command(after_help = after_help_advanced())]
pub struct AdvancedMode {
	#[arg(short, long, value_enum, num_args(0..), help = "Add common types to your config trait from the CLI.")]
	pub(crate) config_common_types: Vec<TemplatePalletConfigCommonTypes>,
	#[arg(short, long, help = "Use a default configuration for your config trait.")]
	pub(crate) default_config: bool,
	#[arg(short, long, value_enum, num_args(0..), help = "Add storage items to your pallet from the CLI.")]
	pub(crate) storage: Vec<TemplatePalletStorageTypes>,
	#[arg(short, long, help = "Add a genesis config to your pallet.")]
	pub(crate) genesis_config: bool,
	#[arg(short = 'o', long, help = "Add a custom origin to your pallet.")]
	pub(crate) custom_origin: bool,
}

impl NewPalletCommand {
	/// Executes the command.
	pub(crate) async fn execute(&self) -> anyhow::Result<()> {
		self.generate_pallet(&mut cli::Cli).await
	}

	/// Generates a pallet
	async fn generate_pallet(&self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Generate a pallet")?;

		let mut pallet_default_config = false;
		let mut pallet_common_types = Vec::new();
		let mut pallet_storage = Vec::new();
		let mut pallet_genesis = false;
		let mut pallet_custom_origin = false;

		if let Some(Mode::Advanced(advanced_mode_args)) = &self.mode {
			if advanced_mode_args.config_common_types.is_empty() &&
				advanced_mode_args.storage.is_empty() &&
				!(advanced_mode_args.genesis_config ||
					advanced_mode_args.default_config ||
					advanced_mode_args.custom_origin)
			{
				cli.info("Generate the pallet's config trait.")?;

				pallet_common_types = multiselect_pick!(
					TemplatePalletConfigCommonTypes,
					"Are you interested in adding one of these types and their usual configuration to your pallet?"
				);
				cli.info("Generate the pallet's storage.")?;

				pallet_storage = multiselect_pick!(
					TemplatePalletStorageTypes,
					"Are you interested in adding some of those storage items to your pallet?"
				);

				// If there's no common types, default_config is excluded from the multiselect
				let boolean_options = if pallet_common_types.is_empty() {
					multiselect_pick!(
						TemplatePalletOptions,
						"Are you interested in adding one of these types and their usual configuration to your pallet?",
						[TemplatePalletOptions::DefaultConfig]
					)
				} else {
					multiselect_pick!(
						TemplatePalletOptions,
						"Are you interested in adding one of these types and their usual configuration to your pallet?"
					)
				};

				pallet_default_config =
					boolean_options.contains(&TemplatePalletOptions::DefaultConfig);
				pallet_genesis = boolean_options.contains(&TemplatePalletOptions::GenesisConfig);
				pallet_custom_origin =
					boolean_options.contains(&TemplatePalletOptions::CustomOrigin);
			} else {
				pallet_common_types.clone_from(&advanced_mode_args.config_common_types);
				pallet_default_config = advanced_mode_args.default_config;
				if pallet_common_types.is_empty() && pallet_default_config {
					return Err(anyhow::anyhow!(
						"Specify at least a config common type to use default config."
					));
				}
				pallet_storage.clone_from(&advanced_mode_args.storage);
				pallet_genesis = advanced_mode_args.genesis_config;
				pallet_custom_origin = advanced_mode_args.custom_origin;
			}
		};

		let pallet_path = if let Some(path) = &self.name {
			PathBuf::from(path)
		} else {
			let path: String = cli
				.input("Where should your project be created?")
				.placeholder("./template")
				.default_input("./template")
				.interact()?;
			PathBuf::from(path)
		};

		// Determine if the pallet is being created inside a workspace
		let workspace_toml = rustilities::manifest::find_workspace_manifest(&pallet_path);
		check_destination_path(&pallet_path, cli)?;

		let spinner = cliclack::spinner();
		spinner.start("Generating pallet...");
		create_pallet_template(
			pallet_path.clone(),
			TemplatePalletConfig {
				authors: self.authors.clone().expect("default values"),
				description: self.description.clone().expect("default values"),
				pallet_in_workspace: workspace_toml.is_some(),
				pallet_advanced_mode: self.mode.is_some(),
				pallet_default_config,
				pallet_common_types,
				pallet_storage,
				pallet_genesis,
				pallet_custom_origin,
			},
		)?;

		// If the pallet has been created inside a workspace, add it to that workspace
		if let Some(workspace_toml) = workspace_toml {
			// Canonicalize paths before passing to rustilities to avoid strip_prefix errors
			// This ensures paths are absolute and consistent, especially when using simple names
			rustilities::manifest::add_crate_to_workspace(
				&workspace_toml.canonicalize()?,
				&pallet_path.canonicalize()?,
			)?;
		}

		// Format the dir. If this fails we do nothing, it's not a major failure
		Command::new("cargo")
			.arg("fmt")
			.arg("--all")
			.current_dir(&pallet_path)
			.output()?;

		spinner.stop("Generation complete");
		cli.info(self.display())?;
		cli.outro(format!(
			"cd into \"{}\" and enjoy hacking! 🚀",
			pallet_path
				.to_str()
				.expect("If the path isn't valid, create_pallet_template detects it; qed")
		))?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut full_message = "pop new pallet".to_string();
		if let Some(name) = &self.name {
			full_message.push_str(&format!(" {}", name));
		}
		if let Some(authors) = &self.authors {
			full_message.push_str(&format!(" --authors \"{}\"", authors));
		}
		if let Some(description) = &self.description {
			full_message.push_str(&format!(" --description \"{}\"", description));
		}
		if let Some(mode) = &self.mode {
			match mode {
				Mode::Advanced(advanced) => {
					full_message.push_str(" advanced");
					for t in &advanced.config_common_types {
						full_message.push_str(&format!(" --config-common-types {:?}", t));
					}
					if advanced.default_config {
						full_message.push_str(" --default-config");
					}
					for s in &advanced.storage {
						full_message.push_str(&format!(" --storage {:?}", s));
					}
					if advanced.genesis_config {
						full_message.push_str(" --genesis-config");
					}
					if advanced.custom_origin {
						full_message.push_str(" --custom-origin");
					}
				},
			}
		}
		full_message
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn test_new_pallet_command_display() {
		let cmd = NewPalletCommand {
			name: Some("my-pallet".to_string()),
			authors: Some("Me".to_string()),
			description: Some("My Pallet".to_string()),
			mode: None,
		};
		assert_eq!(
			cmd.display(),
			"pop new pallet my-pallet --authors \"Me\" --description \"My Pallet\""
		);

		let cmd = NewPalletCommand {
			name: Some("my-pallet".to_string()),
			authors: Some("Me".to_string()),
			description: Some("My Pallet".to_string()),
			mode: Some(Mode::Advanced(AdvancedMode {
				config_common_types: vec![TemplatePalletConfigCommonTypes::RuntimeEvent],
				default_config: true,
				storage: vec![TemplatePalletStorageTypes::StorageValue],
				genesis_config: true,
				custom_origin: true,
			})),
		};
		assert_eq!(
			cmd.display(),
			"pop new pallet my-pallet --authors \"Me\" --description \"My Pallet\" advanced --config-common-types RuntimeEvent --default-config --storage StorageValue --genesis-config --custom-origin"
		);
	}

	#[tokio::test]
	async fn generate_simple_pallet_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let pallet_path = dir.path().join("my-pallet");
		let mut cli = MockCli::new()
			.expect_intro("Generate a pallet")
			.expect_input(
				"Where should your project be created?",
				pallet_path.display().to_string(),
			)
			.expect_outro(format!("cd into {:?} and enjoy hacking! 🚀", pallet_path.display()));

		NewPalletCommand {
			name: None,
			authors: Some("Anonymous".into()),
			description: Some("Frame Pallet".into()),
			mode: None,
		}
		.generate_pallet(&mut cli)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn generate_advanced_pallet_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let pallet_path = dir.path().join("my-pallet");

		let mut cli = MockCli::new()
			.expect_intro("Generate a pallet")
			.expect_input(
				"Where should your project be created?",
				pallet_path.display().to_string(),
			)
			.expect_outro(format!("cd into {:?} and enjoy hacking! 🚀", pallet_path.display()));
		NewPalletCommand {
			name: None,
			authors: Some("Anonymous".into()),
			description: Some("Frame Pallet".into()),
			mode: Some(Mode::Advanced(AdvancedMode {
				config_common_types: vec![TemplatePalletConfigCommonTypes::RuntimeEvent],
				default_config: false,
				storage: vec![TemplatePalletStorageTypes::StorageValue],
				genesis_config: false,
				custom_origin: false,
			})),
		}
		.generate_pallet(&mut cli)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn generate_advanced_pallet_fails_needs_config_common_type() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_intro("Generate a pallet");
		assert!(matches!(NewPalletCommand {
			name: None,
			authors: Some("Anonymous".into()),
			description: Some("Frame Pallet".into()),
			mode: Some(Mode::Advanced(AdvancedMode {
				config_common_types: vec![],
				default_config: true,
				storage: vec![TemplatePalletStorageTypes::StorageValue],
				genesis_config: false,
				custom_origin: false,
			})),
		}
		.generate_pallet(&mut cli)
		.await, anyhow::Result::Err(message) if message.to_string() == "Specify at least a config common type to use default config."));
		cli.verify()
	}

	#[tokio::test]
	async fn test_pallet_in_workspace_with_simple_name() -> anyhow::Result<()> {
		// The bug occurs when you pass a simple name like "my_pallet" instead of "./my_pallet"
		// and the pallet is created inside a workspace.
		let temp_dir = tempdir()?;
		let workspace_path = temp_dir.path();
		// Create a workspace Cargo.toml
		fs::write(
			workspace_path.join("Cargo.toml"),
			r#"[workspace]
resolver = "2"
members = []

[workspace.package]
edition = "2024"
"#,
		)?;
		// Change to the workspace directory to simulate real usage
		let original_dir = std::env::current_dir()?;
		std::env::set_current_dir(workspace_path)?;
		let result = NewPalletCommand {
			name: Some("my_pallet".to_string()),
			authors: Some("Test Author".to_string()),
			description: Some("Test pallet".to_string()),
			mode: None,
		}
		.generate_pallet(&mut MockCli::new())
		.await;
		std::env::set_current_dir(original_dir)?;
		result?;
		// Verify the pallet was created
		assert!(workspace_path.join("my_pallet").exists());
		assert!(workspace_path.join("my_pallet/Cargo.toml").exists());
		assert!(workspace_path.join("my_pallet/src/lib.rs").exists());
		// Verify it was added to the workspace
		let workspace_toml = fs::read_to_string(workspace_path.join("Cargo.toml"))?;
		assert!(
			workspace_toml.contains("my_pallet"),
			"Pallet should be added to workspace members"
		);
		Ok(())
	}
}
