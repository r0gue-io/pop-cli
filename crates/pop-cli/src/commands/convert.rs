// SPDX-License-Identifier: GPL-3.0

use self::Command::*;
use super::*;
use crate::cli::traits::Cli;
use anyhow::{Result, anyhow};
use clap::Args;
use regex::Regex;
use sp_core::{
	bytes::{from_hex, to_hex},
	crypto::{AccountId32, Ss58Codec},
	keccak_256,
};

const ETHEREUM_ADDRESS_REGEX: &str = "^0x[0-9a-fA-F]{40}$";
const PUBLIC_KEY_REGEX: &str = "^0x[0-9a-fA-F]{64}$";
const EE_BYTE: u8 = 0xEE;
const DEFAULT_POLKADOT_SS58_PREFIX: u16 = 0;

fn convert_address(address: &str) -> Result<String> {
	let eth_regex = Regex::new(ETHEREUM_ADDRESS_REGEX)?;
	let pubkey_regex = Regex::new(PUBLIC_KEY_REGEX)?;

	if eth_regex.is_match(address) {
		let mut raw_bytes = from_hex(&address[2..])?;
		raw_bytes.extend_from_slice(&[EE_BYTE; 12]);

		// Convert H256 to AccountId32 first
		let account_id = AccountId32::new(raw_bytes[..].try_into()?);
		let ss58_address =
			account_id.to_ss58check_with_version(DEFAULT_POLKADOT_SS58_PREFIX.into());
		Ok(ss58_address)
	} else {
		// Try to decode SS58 address
		let account_id_32 = if pubkey_regex.is_match(address) {
			AccountId32::new(
				from_hex(address)?.try_into().map_err(|_| anyhow!("Invalid public key"))?,
			)
		} else {
			AccountId32::from_ss58check(address)?
		};
		let public_key_bytes: [u8; 32] = account_id_32.into();

		let address = if public_key_bytes[20..].iter().all(|&b| b == EE_BYTE) {
			// Verify the last 12 bytes are 0xEE and get the first 20 bytes
			to_hex(&public_key_bytes[..20], false)
		} else {
			// Hash the public key with keccak and get the last 20 bytes
			let hash = keccak_256(public_key_bytes.as_ref());
			to_hex(&hash[12..], false)
		};

		Ok(address)
	}
}

/// Arguments for utility commands.
#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct ConvertArgs {
	/// Entry point subcommand for several utility functionalities.
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Entrypoint for several utility commands.
#[derive(Subcommand, Serialize)]
pub(crate) enum Command {
	/// Convert an Ethereum address into a Substrate address and vice versa.
	#[clap(alias = "a")]
	Address {
		#[arg(help = "The Substrate or Ethereum address")]
		address: String,
	},
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<serde_json::Value> {
		match self {
			Address { address } => {
				let output = convert_address(address.as_str())?;
				cli.plain(&output)?;
				Ok(serde_json::json!({
					"input": address,
					"output": output,
				}))
			},
		}
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
			convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e").unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);

		// Test case sensitivity in Ethereum addresses
		assert_eq!(
			convert_address("0x742D35CC6634C0532925A3B844BC454E4438F44E").unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);
		assert_eq!(
			convert_address("0x742d35cc6634c0532925a3b844bc454e4438f44e").unwrap(),
			"13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX"
		);

		// Test invalid Ethereum addresses
		assert!(convert_address("742d35Cc6634C0532925a3b844Bc454e4438f44e").is_err()); // Missing 0x prefix
		assert!(convert_address("0xInvalidAddress").is_err()); // Invalid characters
		assert!(convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44").is_err()); // Too short
		assert!(convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e1").is_err()); // Too long
		assert!(convert_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44g").is_err()); // Invalid hex

		// Test SS58 to ETH conversion with different formats
		assert_eq!(
			convert_address("13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX").unwrap(),
			"0x742d35cc6634c0532925a3b844bc454e4438f44e"
		);
		assert_eq!(
			convert_address("5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6qg").unwrap(),
			"0x742d35cc6634c0532925a3b844bc454e4438f44e"
		);

		// Test native (non-0xEE-formatted) Substrate address to ETH conversion
		assert_eq!(
			convert_address("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty").unwrap(),
			"0x41dccbd49b26c50d34355ed86ff0fa9e489d1e01"
		);
		assert_eq!(
			convert_address("14E5nqKAp3oAJcmzgZhUD2RcptBeUBScxKHgJKU4HPNcKVf3").unwrap(),
			"0x41dccbd49b26c50d34355ed86ff0fa9e489d1e01"
		);
		assert_eq!(
			convert_address("0x8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48")
				.unwrap(),
			"0x41dccbd49b26c50d34355ed86ff0fa9e489d1e01"
		);

		// Invalid Substrate address formats
		assert!(convert_address("invalid_format").is_err()); // Completely invalid format
		assert!(convert_address("5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6q").is_err()); // Too short
		assert!(convert_address("5Eh2qnm8NwCeDnfBhm2aCfoSffBAeMk914NUs8UDGLuoY6qgg").is_err()); // Too long
	}
}
