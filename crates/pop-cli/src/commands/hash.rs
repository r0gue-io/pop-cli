// SPDX-License-Identifier: GPL-3.0

use super::*;
use anyhow::Result;
use clap::builder::PossibleValuesParser;
use sp_core::{
	bytes::{from_hex, to_hex},
	hashing::*,
};
use std::ops::Deref;
use strum_macros::Display;

const CONCAT: &'static str = "Whether to append the source data to the hash.";
const DATA: &'static str =
	"The data to be hashed: input directly or specified as a path to a file.";
const LENGTH: &'static str = "The length of the resulting hash, in bits.";
const MAX_CODE_SIZE: u64 = 3 * 1024 * 1024;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct HashArgs {
	#[command(subcommand)]
	pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
	/// Hashes data using the BLAKE2b cryptographic hash algorithm.
	#[clap(alias = "b2")]
	Blake2 {
		#[arg(help = LENGTH, value_parser = PossibleValuesParser::new(["64", "128", "256", "512"]))]
		length: String,
		#[arg(help = DATA)]
		data: Data,
		#[arg(short, help = CONCAT, long)]
		concat: bool,
	},
	/// Hashes data using the Keccak cryptographic hash algorithm.
	#[clap(alias = "kk")]
	Keccak {
		#[arg(help = LENGTH, value_parser = PossibleValuesParser::new(["256", "512"]))]
		length: String,
		#[arg(help = DATA)]
		data: Data,
	},
	/// Hashes data using the SHA-2 cryptographic hash algorithm.
	#[clap(alias = "s2")]
	Sha2 {
		#[arg(help = LENGTH, value_parser = PossibleValuesParser::new(["256"]))]
		length: String,
		#[arg(help = DATA)]
		data: Data,
	},
	/// Hashes data using the non-cryptographic xxHash hash algorithm.
	#[clap(alias = "xx", name = "twox")]
	TwoX {
		#[arg(help = LENGTH, value_parser = PossibleValuesParser::new(["64", "128"]))]
		length: String,
		#[arg(help = DATA)]
		data: Data,
		#[arg(short, help = CONCAT, long)]
		concat: bool,
	},
}

impl Command {
	pub(crate) fn execute(&self) -> Result<()> {
		let (hash, data) = match self {
			Command::Blake2 { length, data, concat } => {
				let mut hash = match length.parse::<u16>()? {
					64 => blake2_64(data).to_vec(),
					128 => blake2_128(data).to_vec(),
					256 => blake2_256(data).to_vec(),
					512 => blake2_512(data).to_vec(),
					_ => unreachable!("args validated by clap"),
				};
				if *concat {
					hash.extend_from_slice(data)
				}
				(hash, data)
			},
			Command::Keccak { length, data } => {
				let hash = match length.parse::<u16>()? {
					256 => keccak_256(data).to_vec(),
					512 => keccak_512(data).to_vec(),
					_ => unreachable!("args validated by clap"),
				};
				(hash, data)
			},
			Command::Sha2 { length, data } => {
				let hash = match length.parse::<u16>()? {
					256 => sha2_256(data).to_vec(),
					_ => unreachable!("args validated by clap"),
				};
				(hash, data)
			},
			Command::TwoX { length, data, concat } => {
				let mut hash = match length.parse::<u16>()? {
					64 => twox_64(data).to_vec(),
					128 => twox_128(data).to_vec(),
					256 => twox_256(data).to_vec(),
					_ => unreachable!("args validated by clap"),
				};
				if *concat {
					hash.extend_from_slice(data)
				}
				(hash, data)
			},
		};

		println!("{} {}", to_hex(&hash, false), console::style(format!("(Source: {data})")).dim());
		Ok(())
	}
}

#[derive(Clone, Debug, Display)]
pub(crate) enum Data {
	File(Vec<u8>),
	Hex(Vec<u8>),
	String(Vec<u8>),
}

impl From<&str> for Data {
	fn from(value: &str) -> Self {
		// Check if value is specifying a file
		if let Ok(metadata) = std::fs::metadata(value) {
			if metadata.is_file() {
				// Limit the size to that of the max code size for a runtime
				if metadata.len() > MAX_CODE_SIZE {
					panic!("file size exceeds maximum code size");
				}

				if let Ok(data) = std::fs::read(value) {
					return Self::File(data)
				}
			}
		}
		// Otherwise check if hex via prefix or just hash as string
		if value.starts_with("0x") {
			Self::Hex(from_hex(value).unwrap())
		} else {
			Self::String(value.as_bytes().into())
		}
	}
}

impl Deref for Data {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		match self {
			Data::File(data) => data,
			Data::Hex(data) => data,
			Data::String(data) => data,
		}
	}
}
