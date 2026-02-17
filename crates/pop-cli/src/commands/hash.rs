// SPDX-License-Identifier: GPL-3.0

use self::Command::*;
use super::*;
use crate::{
	cli::traits::Cli,
	output::{CliResponse, OutputMode},
};
use anyhow::{Result, anyhow};
use clap::{
	Arg, Args, Error,
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
};
use sp_core::{
	bytes::{from_hex, to_hex},
	hashing::*,
};
use std::{ffi::OsStr, ops::Deref, str::FromStr};
use strum_macros::Display;

const CONCAT: &str = "Whether to append the source data to the hash.";
const DATA: &str = "The data to be hashed: input directly or specified as a path to a file.";
const LENGTH: &str = "The length of the resulting hash, in bits.";
const MAX_CODE_SIZE: u64 = 3 * 1024 * 1024;

/// Arguments for hashing.
#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct HashArgs {
	/// Hash data using a supported hash algorithm.
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Hash data using a supported hash algorithm.
#[derive(Subcommand, Serialize)]
pub(crate) enum Command {
	/// Hashes data using the BLAKE2b cryptographic hash algorithm.
	#[clap(alias = "b2")]
	Blake2 {
		#[arg(help = LENGTH, value_parser = SupportedLengths::new([64, 128, 256, 512]))]
		length: u16,
		#[arg(help = DATA)]
		data: Data,
		#[arg(short, help = CONCAT, long)]
		concat: bool,
	},
	/// Hashes data using the Keccak cryptographic hash algorithm.
	#[clap(alias = "kk")]
	Keccak {
		#[arg(help = LENGTH, value_parser = SupportedLengths::new([256, 512]))]
		length: u16,
		#[arg(help = DATA)]
		data: Data,
	},
	/// Hashes data using the SHA-2 cryptographic hash algorithm.
	#[clap(alias = "s2")]
	Sha2 {
		#[arg(help = LENGTH, value_parser = SupportedLengths::new([256]))]
		length: u16,
		#[arg(help = DATA)]
		data: Data,
	},
	/// Hashes data using the non-cryptographic xxHash hash algorithm.
	#[clap(alias = "xx", name = "twox")]
	TwoX {
		#[arg(help = LENGTH, value_parser = SupportedLengths::new([64, 128, 256]))]
		length: u16,
		#[arg(help = DATA)]
		data: Data,
		#[arg(short, help = CONCAT, long)]
		concat: bool,
	},
}

/// Structured output for JSON mode.
#[derive(serde::Serialize)]
struct HashOutput {
	algorithm: String,
	length: u16,
	hash: String,
}

/// Entry point called from the command dispatcher.
pub(crate) fn execute(command: &Command, output_mode: OutputMode) -> Result<()> {
	match output_mode {
		OutputMode::Human => command.execute(&mut crate::cli::Cli),
		OutputMode::Json => {
			let (algorithm, length) = command.algorithm_info();
			let hash_hex = to_hex(&command.hash()?, false);
			let output = HashOutput { algorithm, length, hash: hash_hex };
			CliResponse::ok(output).print_json();
			Ok(())
		},
	}
}

impl Command {
	/// Executes the command in human mode.
	pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<()> {
		let output = &to_hex(&self.hash()?, false)[2..];
		cli.plain(output)?;
		Ok(())
	}

	/// Returns the (algorithm name, bit length) for the command.
	fn algorithm_info(&self) -> (String, u16) {
		match self {
			Blake2 { length, .. } => ("blake2".into(), *length),
			Keccak { length, .. } => ("keccak".into(), *length),
			Sha2 { length, .. } => ("sha2".into(), *length),
			TwoX { length, .. } => ("twox".into(), *length),
		}
	}

	fn hash(&self) -> Result<Vec<u8>> {
		match self {
			Blake2 { length, data, concat } => {
				let mut hash = match length {
					64 => blake2_64(data).to_vec(),
					128 => blake2_128(data).to_vec(),
					256 => blake2_256(data).to_vec(),
					512 => blake2_512(data).to_vec(),
					_ => return Err(anyhow!("unsupported length: {}", length)),
				};
				if *concat {
					hash.extend_from_slice(data)
				}
				Ok(hash)
			},
			Keccak { length, data } => {
				let hash = match length {
					256 => keccak_256(data).to_vec(),
					512 => keccak_512(data).to_vec(),
					_ => return Err(anyhow!("unsupported length: {}", length)),
				};
				Ok(hash)
			},
			Sha2 { length, data } => {
				let hash = match length {
					256 => sha2_256(data).to_vec(),
					_ => return Err(anyhow!("unsupported length: {}", length)),
				};
				Ok(hash)
			},
			TwoX { length, data, concat } => {
				let mut hash = match length {
					64 => twox_64(data).to_vec(),
					128 => twox_128(data).to_vec(),
					256 => twox_256(data).to_vec(),
					_ => return Err(anyhow!("unsupported length: {}", length)),
				};
				if *concat {
					hash.extend_from_slice(data)
				}
				Ok(hash)
			},
		}
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Blake2 { length, .. } => write!(f, "blake2 {length}"),
			Keccak { length, .. } => write!(f, "keccak {length}"),
			Sha2 { length, .. } => write!(f, "sha2 {length}"),
			TwoX { length, .. } => write!(f, "twox {length}"),
		}
	}
}

#[derive(Clone, Debug, Display, Eq, PartialEq, Serialize)]
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

#[derive(Clone)]
struct SupportedLengths(PossibleValuesParser);
impl SupportedLengths {
	fn new<const N: usize>(values: [u16; N]) -> Self {
		Self(PossibleValuesParser::new(values.map(|l| PossibleValue::new(l.to_string()))))
	}
}
impl TypedValueParser for SupportedLengths {
	type Value = u16;

	fn parse_ref(
		&self,
		cmd: &clap::Command,
		arg: Option<&Arg>,
		value: &OsStr,
	) -> std::result::Result<Self::Value, Error> {
		self.0
			.parse_ref(cmd, arg, value)
			.map(|v| v.parse::<u16>().expect("only u16 values supported"))
	}

	fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
		self.0.possible_values()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::output::OutputMode;
	use Data::*;
	use std::io::Write;

	#[test]
	fn blake2_works() -> Result<()> {
		let data = "test".as_bytes();
		for (length, expected) in [
			(64u16, &blake2_64(data)[..]),
			(128, &blake2_128(data)[..]),
			(256, &blake2_256(data)[..]),
			(512, &blake2_512(data)[..]),
		] {
			for data in [File(data.to_vec()), Hex(data.to_vec()), String(data.to_vec())] {
				for concat in [false, true] {
					let expected = match concat {
						true => [expected, data.as_ref()].concat(),
						false => expected.to_vec(),
					};
					let command = Blake2 { length, data: data.clone(), concat };
					assert_eq!(command.hash()?, expected, "hash using {} failed", command);
				}
			}
		}
		Ok(())
	}

	#[test]
	fn blake2_unsupported_length_fails() {
		for length in [0, 8, 16, 32, 1024, u16::MAX] {
			let command = Blake2 { length, data: None, concat: false };
			assert_eq!(
				format!("{}", command.hash().unwrap_err().root_cause()),
				format!("unsupported length: {length}")
			);
		}
	}

	#[test]
	fn keccak_works() -> Result<()> {
		let data = "test".as_bytes();
		for (length, expected) in [(256, &keccak_256(data)[..]), (512, &keccak_512(data)[..])] {
			for data in [File(data.to_vec()), Hex(data.to_vec()), String(data.to_vec())] {
				let command = Keccak { length, data: data.clone() };
				assert_eq!(command.hash()?, expected, "hash using {} failed", command);
			}
		}
		Ok(())
	}

	#[test]
	fn keccak_unsupported_length_fails() {
		for length in [0, 8, 16, 32, 64, 128, 1024, u16::MAX] {
			let command = Keccak { length, data: None };
			assert_eq!(
				format!("{}", command.hash().unwrap_err().root_cause()),
				format!("unsupported length: {length}")
			);
		}
	}

	#[test]
	fn sha2_works() -> Result<()> {
		let data = "test".as_bytes();
		for (length, expected) in [(256, sha2_256(data))] {
			for data in [File(data.to_vec()), Hex(data.to_vec()), String(data.to_vec())] {
				let command = Sha2 { length, data: data.clone() };
				assert_eq!(command.hash()?, expected, "hash using {} failed", command);
			}
		}
		Ok(())
	}

	#[test]
	fn sha2_unsupported_length_fails() {
		for length in [0, 8, 16, 32, 64, 128, 512, 1024, u16::MAX] {
			let command = Sha2 { length, data: None };
			assert_eq!(
				format!("{}", command.hash().unwrap_err().root_cause()),
				format!("unsupported length: {length}")
			);
		}
	}

	#[test]
	fn twox_works() -> Result<()> {
		let data = "test".as_bytes();
		for (length, expected) in
			[(64u16, &twox_64(data)[..]), (128, &twox_128(data)[..]), (256, &twox_256(data)[..])]
		{
			for data in [File(data.to_vec()), Hex(data.to_vec()), String(data.to_vec())] {
				for concat in [false, true] {
					let expected = match concat {
						true => [expected, data.as_ref()].concat(),
						false => expected.to_vec(),
					};
					let command = TwoX { length, data: data.clone(), concat };
					assert_eq!(command.hash()?, expected, "hash using {} failed", command);
				}
			}
		}
		Ok(())
	}

	#[test]
	fn twox_unsupported_length_fails() {
		for length in [0, 8, 16, 32, 512, 1024, u16::MAX] {
			let command = TwoX { length, data: None, concat: false };
			assert_eq!(
				format!("{}", command.hash().unwrap_err().root_cause()),
				format!("unsupported length: {length}")
			);
		}
	}

	#[test]
	fn command_display_works() {
		let blake2 = [64, 128, 256, 512].into_iter().map(|length| {
			(Blake2 { length, data: Data::default(), concat: false }, format!("blake2 {length}"))
		});
		let keccak = [256, 512]
			.into_iter()
			.map(|length| (Keccak { length, data: Data::default() }, format!("keccak {length}")));
		let sha2 = [256]
			.into_iter()
			.map(|length| (Sha2 { length, data: Data::default() }, format!("sha2 {length}")));
		let twox = [64, 128].into_iter().map(|length| {
			(TwoX { length, data: Data::default(), concat: false }, format!("twox {length}"))
		});

		for (command, expected) in blake2.chain(keccak).chain(sha2).chain(twox) {
			assert_eq!(command.to_string(), expected);
		}
	}

	#[test]
	fn data_from_invalid_path_treated_as_string() {
		let file = "./path/to/file";
		assert!(matches!(Data::from_str(file), Ok(String(bytes)) if bytes == file.as_bytes()));
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
		let bytes_written = file.write(value)?;
		assert_eq!(bytes_written, value.len());
		assert!(
			matches!(Data::from_str(file.path().to_str().unwrap()), Ok(File(bytes)) if bytes == value)
		);
		Ok(())
	}

	#[test]
	fn data_from_hex_string_works() {
		let value = "test".as_bytes();
		let hex = to_hex(value, true);
		assert!(matches!(Data::from_str(hex.as_str()), Ok(Hex(bytes)) if bytes == value));
	}

	#[test]
	fn data_from_string_works() {
		let value = "test";
		assert!(matches!(Data::from_str(value), Ok(String(bytes)) if bytes == value.as_bytes()));
	}

	#[test]
	fn supported_lengths_works() {
		let values = [8, 16, 32, 64];
		let supported_lengths = SupportedLengths::new(values);
		for value in values {
			assert_eq!(
				supported_lengths
					.parse_ref(&Default::default(), Option::None, OsStr::new(&value.to_string()))
					.unwrap(),
				value
			);
		}
		assert!(
			supported_lengths
				.possible_values()
				.unwrap()
				.eq(values.map(|v| PossibleValue::new(v.to_string())),)
		)
	}

	#[test]
	fn execute_human_mode_works() -> Result<()> {
		use crate::cli::MockCli;
		let data = "test".as_bytes();
		let hash_hex = to_hex(blake2_256(data).as_ref(), false);
		let expected = &hash_hex[2..];
		let command = Blake2 { length: 256, data: String(data.to_vec()), concat: false };
		let mut cli = MockCli::new().expect_plain(expected);
		command.execute(&mut cli)?;
		cli.verify()
	}

	#[test]
	fn execute_json_mode_works() -> Result<()> {
		let command =
			Blake2 { length: 256, data: String("test".as_bytes().to_vec()), concat: false };
		// Should not panic; JSON is printed to stdout.
		execute(&command, OutputMode::Json)
	}

	#[test]
	fn algorithm_info_works() {
		assert_eq!(
			Blake2 { length: 256, data: None, concat: false }.algorithm_info(),
			("blake2".to_string(), 256)
		);
		assert_eq!(
			Keccak { length: 512, data: None }.algorithm_info(),
			("keccak".to_string(), 512)
		);
		assert_eq!(Sha2 { length: 256, data: None }.algorithm_info(), ("sha2".to_string(), 256));
		assert_eq!(
			TwoX { length: 128, data: None, concat: false }.algorithm_info(),
			("twox".to_string(), 128)
		);
	}
}
