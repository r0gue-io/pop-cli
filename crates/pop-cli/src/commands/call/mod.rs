// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result};

#[cfg(feature = "chain")]
pub(crate) mod chain;
#[cfg(feature = "contract")]
pub(crate) mod contract;

/// Arguments for calling a smart contract.
#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CallArgs {
	#[command(subcommand)]
	pub command: Option<Command>,
}

/// Call a chain or a smart contract.
#[derive(Subcommand, Serialize, Clone)]
pub(crate) enum Command {
	/// Call a chain
	#[cfg(feature = "chain")]
	#[clap(aliases = ["C", "p", "parachain"])]
	Chain(chain::CallChainCommand),
	/// Call a contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}

/// Structured JSON output for `pop --json call ...`.
#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "target")]
pub(crate) enum CallJsonOutput {
	#[cfg(feature = "chain")]
	Chain(chain::CallChainOutput),
	#[cfg(feature = "contract")]
	Contract(contract::CallContractOutput),
}

impl CallArgs {
	/// Auto-detects the project type and returns the appropriate command if none was specified.
	pub(crate) fn resolve_command(&self) -> anyhow::Result<Command> {
		if let Some(command) = &self.command {
			return Ok(command.clone());
		}

		// Auto-detect project type based on current directory
		#[cfg(feature = "contract")]
		{
			let current_dir = std::env::current_dir()?;
			if matches!(pop_contracts::is_supported(&current_dir), Ok(true)) {
				let mut cmd = contract::CallContractCommand::default();
				cmd.path_pos = Some(current_dir);
				return Ok(Command::Contract(cmd));
			}
		}

		#[cfg(feature = "chain")]
		return Ok(Command::Chain(Default::default()));

		#[cfg(not(feature = "chain"))]
		Err(anyhow::anyhow!(
			"Could not detect project type. Please specify 'chain' or 'contract' explicitly, \
			or ensure you are in a valid contract or chain project directory."
		))
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "chain")]
			Command::Chain(_) => write!(f, "chain"),
			#[cfg(feature = "contract")]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pop_common::helpers::with_current_dir;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::Chain(Default::default()).to_string(), "chain");
		#[cfg(feature = "contract")]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}

	#[cfg(feature = "chain")]
	#[test]
	fn resolve_command_with_inner_chain_command_should_work() {
		matches!(
			CallArgs { command: Some(Command::Chain(Default::default())) }.resolve_command(),
			Ok(Command::Chain(..))
		);
	}

	#[cfg(feature = "contract")]
	#[test]
	fn resolve_command_with_inner_contract_command_should_work() {
		matches!(
			CallArgs { command: Some(Command::Contract(Default::default())) }.resolve_command(),
			Ok(Command::Contract(..))
		);
	}

	#[cfg(feature = "chain")]
	#[test]
	fn resolve_command_in_directory_with_chain_should_work() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let cargo_toml = r#"[package]
name = "test-chain"
version = "0.1.0"

[dependencies]
substrate-frame-rpc-system = "4.0.0"
parity-scale-codec = "3.0.0"
"#;
		fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml)?;
		with_current_dir(temp_dir.as_ref(), || {
			matches!(CallArgs { command: None }.resolve_command(), Ok(Command::Chain(..)));
			Ok(())
		})
	}

	#[cfg(feature = "contract")]
	#[test]
	fn resolve_command_in_directory_with_contract_should_work() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let cargo_toml = r#"[package]
name = "test-contract"
version = "0.1.0"

[dependencies]
ink = "5.1.1"
"#;
		fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml)?;
		with_current_dir(temp_dir.as_ref(), || {
			matches!(CallArgs { command: None }.resolve_command(), Ok(Command::Contract(..)));
			Ok(())
		})
	}

	#[test]
	fn resolve_command_in_directory_with_nothing_should_work() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Try without Cargo.toml file
		with_current_dir(temp_dir.as_ref(), || {
			assert!(matches!(CallArgs { command: None }.resolve_command()?, Command::Chain(_)));
			Ok(())
		})
	}
}
