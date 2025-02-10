// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use std::{
	fs::{self, OpenOptions},
	io::{self, stdin, stdout, Write},
	path::Path,
};

pub(crate) fn sanitize(target: &Path) -> Result<(), Error> {
	if target.exists() {
		print!("\"{}\" directory exists. Do you want to clean it? [y/n]: ", target.display());
		stdout().flush()?;

		let mut input = String::new();
		stdin().read_line(&mut input)?;

		if input.trim().to_lowercase() == "y" {
			fs::remove_dir_all(target).map_err(|_| Error::Aborted)?;
		} else {
			return Err(Error::Aborted);
		}
	}
	Ok(())
}

/// Check if the initial endowment input by the user is a valid balance.
///
/// # Arguments
///
/// * `initial_endowment` - initial endowment amount to be checked for validity.
pub fn is_initial_endowment_valid(initial_endowment: &str) -> bool {
	initial_endowment.parse::<u128>().is_ok() ||
		is_valid_bitwise_left_shift(initial_endowment).is_ok()
}

// Auxiliary method to check if the endowment input with a shift left (1u64 << 60) format is valid.
// Parse the self << rhs format and check the shift left operation is valid.
fn is_valid_bitwise_left_shift(initial_endowment: &str) -> Result<u128, Error> {
	let v: Vec<&str> = initial_endowment.split(" << ").collect();
	if v.len() < 2 {
		return Err(Error::EndowmentError);
	}
	let left = v[0]
		.split('u') // parse 1u64 characters
		.take(1)
		.collect::<String>()
		.parse::<u128>()
		.map_err(|_e| Error::EndowmentError)?;
	let right = v[1]
		.chars()
		.filter(|c| c.is_numeric()) // parse 1u64 characters
		.collect::<String>()
		.parse::<u32>()
		.map_err(|_e| Error::EndowmentError)?;
	left.checked_shl(right).ok_or(Error::EndowmentError)
}

pub(crate) fn write_to_file(path: &Path, contents: &str) -> Result<(), Error> {
	let mut file = OpenOptions::new()
		.write(true)
		.truncate(true)
		.create(true)
		.open(path)
		.map_err(Error::RustfmtError)?;

	file.write_all(contents.as_bytes()).map_err(Error::RustfmtError)?;

	if path.extension().map_or(false, |ext| ext == "rs") {
		let output = std::process::Command::new("rustfmt")
			.arg(path.to_str().unwrap())
			.output()
			.map_err(Error::RustfmtError)?;

		if !output.status.success() {
			return Err(Error::RustfmtError(io::Error::new(
				io::ErrorKind::Other,
				"rustfmt exited with non-zero status code",
			)));
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::generator::parachain::ChainSpec;
	use askama::Template;
	use tempfile::tempdir;

	#[test]
	fn test_write_to_file() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempdir()?;
		let chainspec = ChainSpec {
			token_symbol: "DOT".to_string(),
			decimals: 6,
			initial_endowment: "1000000".to_string(),
		};
		let file_path = temp_dir.path().join("file.rs");
		let _ = fs::write(&file_path, "");
		write_to_file(&file_path, chainspec.render().expect("infallible").as_ref())?;
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("file.rs")).expect("Failed to read file");

		assert!(generated_file_content
			.contains("properties.insert(\"tokenSymbol\".into(), \"DOT\".into());"));
		assert!(generated_file_content
			.contains("properties.insert(\"tokenDecimals\".into(), 6.into());"));
		assert!(generated_file_content.contains("1000000"));

		Ok(())
	}

	#[test]
	fn test_is_initial_endowment_valid() {
		assert_eq!(is_initial_endowment_valid("100000"), true);
		assert_eq!(is_initial_endowment_valid("1u64 << 60"), true);
		assert_eq!(is_initial_endowment_valid("wrong"), false);
		assert_eq!(is_initial_endowment_valid(" "), false);
	}

	#[test]
	fn test_left_shift() {
		// Values from https://stackoverflow.com/questions/56392875/how-can-i-initialize-a-users-balance-in-a-substrate-blockchain
		assert_eq!(is_valid_bitwise_left_shift("1 << 60").unwrap(), 1152921504606846976);
		let result = is_valid_bitwise_left_shift("wrong");
		assert!(result.is_err());
	}
}
