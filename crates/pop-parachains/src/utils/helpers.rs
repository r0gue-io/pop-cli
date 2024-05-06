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

pub fn is_initial_endowment_valid(initial_endowment: String) -> bool {
	match initial_endowment.parse::<i128>() {
		Ok(_) => return true,
		Err(error) => {
			println!("{:?}", error);
			return false;
		},
	}
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
		assert_eq!(is_initial_endowment_valid("100000".to_string()), true);
		//assert_eq!(is_initial_endowment_valid("1u64 << 60".to_string()), true);
		assert_eq!(is_initial_endowment_valid("1 << 60".to_string()), true);
		// assert_eq!(is_initial_endowment_valid("wrong".to_string()), false);
		// assert_eq!(is_initial_endowment_valid(" ".to_string()), false);
	}
}
