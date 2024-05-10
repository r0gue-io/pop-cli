// SPDX-License-Identifier: GPL-3.0
use std::{
	fs::{self, OpenOptions},
	io::{self, stdin, stdout, Write},
	path::Path,
};

use crate::errors::Error;

pub(crate) fn sanitize(target: &Path) -> Result<(), Error> {
	if target.exists() {
		print!("\"{}\" folder exists. Do you want to clean it? [y/n]: ", target.display());
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

pub fn is_initial_endowment_valid(initial_endowment: &str) -> bool {
	initial_endowment.parse::<u128>().is_ok()
		|| is_valid_bitwise_left_shift(initial_endowment).is_ok()
}
// Auxiliar method to check if the endowment input with a shift left (1u64 << 60) format is valid.
// Parse the self << rhs format and check the shift left operation is valid.
fn is_valid_bitwise_left_shift(initial_endowment: &str) -> Result<u128, Error> {
	let v: Vec<&str> = initial_endowment.split(" << ").collect();
	if v.len() < 2 {
		return Err(Error::EndowmentError);
	}
	let left = v[0]
		.split("u") // parse 1u64 characters
		.take(1)
		.collect::<String>()
		.parse::<u128>()
		.or_else(|_e| Err(Error::EndowmentError))?;
	let right = v[1]
		.chars()
		.filter(|c| c.is_numeric()) // parse 1u64 characters
		.collect::<String>()
		.parse::<u32>()
		.or_else(|_e| Err(Error::EndowmentError))?;
	left.checked_shl(right).ok_or(Error::EndowmentError)
}

pub(crate) fn write_to_file(path: &Path, contents: &str) -> Result<(), Error> {
	let mut file = OpenOptions::new()
		.write(true)
		.truncate(true)
		.create(true)
		.open(path)
		.map_err(|err| Error::RustfmtError(err))?;

	file.write_all(contents.as_bytes()).map_err(|err| Error::RustfmtError(err))?;

	if path.extension().map_or(false, |ext| ext == "rs") {
		let output = std::process::Command::new("rustfmt")
			.arg(path.to_str().unwrap())
			.output()
			.map_err(|err| Error::RustfmtError(err))?;

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
