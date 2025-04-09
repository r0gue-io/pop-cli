// SPDX-License-Identifier: GPL-3.0

use super::*;
use anyhow::{anyhow, Result};
use clap::{builder::PossibleValuesParser, Args};
use sp_core::{
	bytes::{from_hex, to_hex},
	hashing::*,
};
use std::{ops::Deref, str::FromStr};
use strum_macros::Display;

const CONCAT: &'static str = "Whether to append the source data to the hash.";
const DATA: &'static str =
	"The data to be hashed: input directly or specified as a path to a file.";
const LENGTH: &'static str = "The length of the resulting hash, in bits.";
const MAX_CODE_SIZE: u64 = 3 * 1024 * 1024;

/// Arguments for hashing.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct HashArgs {
	/// Hash data using a supported hash algorithm.
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Hash data using a supported hash algorithm.
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
	/// Executes the command.
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

		println!(
			"{} {}",
			to_hex(&hash, false),
			console::style(format!("(Source: {data}, Output: {} bytes)", hash.len())).dim()
		);
		Ok(())
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Command::Blake2 { length, .. } => write!(f, "blake2 {length}"),
			Command::Keccak { length, .. } => write!(f, "keccak {length}"),
			Command::Sha2 { length, .. } => write!(f, "sha2 {length}"),
			Command::TwoX { length, .. } => write!(f, "twox {length}"),
		}
	}
}

#[derive(Clone, Debug, Display, Eq, PartialEq)]
#[cfg_attr(test, derive(Default))]
pub(crate) enum Data {
	File(Vec<u8>),
	Hex(Vec<u8>),
	String(Vec<u8>),
	#[cfg(test)]
	#[default]
	None,
}

impl FromStr for Data {
	type Err = anyhow::Error;

	fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
		// Check if value is specifying a file
		if let Ok(metadata) = std::fs::metadata(value) {
			if !metadata.is_file() {
				return Err(anyhow!("specified path is not a file"));
			}
			// Limit the size to that of the max code size for a runtime
			if metadata.len() > MAX_CODE_SIZE {
				return Err(anyhow!("file size exceeds maximum code size"));
			}

			return Ok(Self::File(std::fs::read(value)?));
		}
		// Otherwise check if hex via prefix or just hash as string
		if value.starts_with("0x") {
			Ok(Self::Hex(from_hex(value)?))
		} else {
			Ok(Self::String(value.as_bytes().into()))
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
			#[cfg(test)]
			Data::None => Default::default(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn command_display_works() {
		use Command::*;

		let blake2 = ["64", "128", "256", "512"].iter().map(|len| {
			(
				Blake2 { length: len.to_string(), data: Data::default(), concat: false },
				format!("blake2 {len}"),
			)
		});
		let keccak = ["256", "512"].iter().map(|len| {
			(Keccak { length: len.to_string(), data: Data::default() }, format!("keccak {len}"))
		});
		let sha2 = ["256"].iter().map(|len| {
			(Sha2 { length: len.to_string(), data: Data::default() }, format!("sha2 {len}"))
		});
		let twox = ["64", "128"].iter().map(|len| {
			(
				TwoX { length: len.to_string(), data: Data::default(), concat: false },
				format!("twox {len}"),
			)
		});

		for (command, expected) in blake2.chain(keccak).chain(sha2).chain(twox) {
			assert_eq!(command.to_string(), expected);
		}
	}

	#[test]
	fn data_from_invalid_path_treated_as_string() {
		let file = "./path/to/file";
		assert!(
			matches!(Data::from_str(file), Ok(Data::String(bytes)) if bytes == file.as_bytes())
		);
	}

	#[test]
	fn data_from_file_returns_error_when_directory_specified() {
		assert_eq!(
			format!("{}", Data::from_str("./").unwrap_err().root_cause()),
			"specified path is not a file"
		);
	}

	#[test]
	fn data_from_file_returns_error_when_limit_exceeded() {
		let mut file = tempfile::NamedTempFile::new().unwrap();
		file.write_all(&[0u8; MAX_CODE_SIZE as usize + 1]).unwrap();
		assert_eq!(
			format!("{}", Data::from_str(file.path().to_str().unwrap()).unwrap_err().root_cause()),
			"file size exceeds maximum code size"
		);
	}

	#[test]
	fn data_from_file_works() -> Result<(), Box<dyn std::error::Error>> {
		let value = "test".as_bytes();
		let mut file = tempfile::NamedTempFile::new()?;
		file.write(value)?;
		assert!(
			matches!(Data::from_str(file.path().to_str().unwrap()), Ok(Data::File(bytes)) if bytes == value)
		);
		Ok(())
	}

	#[test]
	fn data_from_hex_string_works() {
		let value = "test".as_bytes();
		let hex = to_hex(value, true);
		assert!(matches!(Data::from_str(hex.as_str()), Ok(Data::Hex(bytes)) if bytes == value));
	}

	#[test]
	fn data_from_string_works() {
		let value = "test";
		assert!(
			matches!(Data::from_str(value), Ok(Data::String(bytes)) if bytes == value.as_bytes())
		);
	}
}
