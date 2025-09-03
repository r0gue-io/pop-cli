// SPDX-License-Identifier: GPL-3.0

use self::Command::*;
use super::*;
use crate::cli::traits::Cli;
use anyhow::Result;
use clap::Args;
use regex::Regex;
use sp_core::{
	bytes::{from_hex, to_hex},
	crypto::{AccountId32, Ss58Codec},
};

const ETHEREUM_ADDRESS_REGEX: &str = "^0x[0-9a-fA-F]{40}$";
const EE_BYTE: u8 = 0xEE;
const DEFAULT_POLKADOT_SS58_PREFIX: u16 = 0;

fn convert_address(address: &str, ss58_prefix: Option<u16>) -> Result<String> {
	let eth_regex = Regex::new(ETHEREUM_ADDRESS_REGEX)?;

	if eth_regex.is_match(address) {
		let mut raw_bytes = from_hex(&address[2..])?;
		raw_bytes.extend_from_slice(&[EE_BYTE; 12]);

		// Convert H256 to AccountId32 first
		let account_id = AccountId32::new(raw_bytes[..].try_into()?);
		let version = ss58_prefix.unwrap_or(DEFAULT_POLKADOT_SS58_PREFIX);
		let ss58_address = account_id.to_ss58check_with_version(version.into());
		Ok(ss58_address)
	} else {
		// Try to decode SS58 address
		let account_id = AccountId32::from_ss58check(address)?;
		let bytes: [u8; 32] = account_id.into();

		// Verify the last 12 bytes are 0xEE
		if !bytes[20..].iter().all(|&b| b == EE_BYTE) {
			return Err(anyhow::anyhow!("Invalid address: last 12 bytes must be 0xEE"));
		}

		// Take the first 20 bytes and format as hex
		let eth_address = to_hex(&bytes[..20], false);
		Ok(eth_address)
	}
}

/// Arguments for utility commands.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct ConvertArgs {
	/// Entry point subcommand for several utility functionalities.
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Entrypoint for several utility commands.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Convert an Ethereum address into a Substrate address and vice versa.
	#[clap(alias = "a")]
	Address {
		#[arg(help = "The Substrate or Ethereum address")]
		address: String,
		#[arg(help = "The SS58 prefix. Defaults to 0 (Polkadot).")]
		prefix: Option<u16>,
	},
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<()> {
		let output = match self {
			Address { address, prefix } => convert_address(address.as_str(), *prefix)?,
		};
		cli.plain(&output)?;
		Ok(())
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Address { address, .. } => write!(f, "{address}"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_ethereum_address_validation() {
		// Test valid Ethereum address conversion with different prefixes
		assert_eq!(
			convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e", Some(0)).unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);
		assert_eq!(
			convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e", None).unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);
		assert_eq!(
			convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e", Some(42)).unwrap(),
			"5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6qg"
		);

		// Test case sensitivity in Ethereum addresses
		assert_eq!(
			convert_address("0x742D35CC6634C0532925A3B844BC454E4438F44E", None).unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);
		assert_eq!(
			convert_address("0x742d35cc6634c0532925a3b844bc454e4438f44e", None).unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);

		// Test invalid Ethereum addresses
		assert!(convert_address("742d35Cc6634C0532925a3b844Bc454e4438f44e", None).is_err()); // Missing 0x prefix
		assert!(convert_address("0xInvalidAddress", None).is_err()); // Invalid characters
		assert!(convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44", None).is_err()); // Too short
		assert!(convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e1", None).is_err()); // Too long
		assert!(convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44g", None).is_err()); // Invalid hex

		// Test SS58 to ETH conversion with different formats
		assert_eq!(
			convert_address("13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX", None).unwrap(),
			"0x742d35cc6634c0532925a3b844bc454e4438f44e"
		);
		assert_eq!(
			convert_address("5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6qg", None).unwrap(),
			"0x742d35cc6634c0532925a3b844bc454e4438f44e"
		);

		// Test invalid SS58 addresses
		let invalid_ss58 = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"; // Not originally from ETH
		assert!(convert_address(invalid_ss58, None).is_err());
		assert!(convert_address("invalid_format", None).is_err()); // Completely invalid format
		assert!(convert_address("5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6q", None).is_err()); // Too short
		assert!(convert_address("5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6qgg", None).is_err()); // Too long
	}
}
