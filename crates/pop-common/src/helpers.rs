// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use regex::RegexBuilder;
use std::{
	collections::HashMap,
	fs,
	io::{Read, Write},
	path::{Path, PathBuf},
};

/// Replaces occurrences of specified strings in a file with new values.
///
/// # Arguments
///
/// * `file_path` - A `PathBuf` specifying the path to the file to be modified.
/// * `replacements` - A `HashMap` where each key-value pair represents a target string and its
///   corresponding replacement string.
pub fn replace_in_file(file_path: PathBuf, replacements: HashMap<&str, &str>) -> Result<(), Error> {
	// Read the file content
	let mut file_content = String::new();
	fs::File::open(&file_path)?.read_to_string(&mut file_content)?;
	// Perform the replacements
	let mut modified_content = file_content;
	for (target, replacement) in &replacements {
		modified_content = modified_content.replace(target, replacement);
	}
	// Write the modified content back to the file
	let mut file = fs::File::create(&file_path)?;
	file.write_all(modified_content.as_bytes())?;
	Ok(())
}

/// Gets the last component (name of a project) of a path or returns a default value if the path has
/// no valid last component.
///
/// # Arguments
/// * `path` - Location path of the project.
/// * `default` - The default string to return if the path has no valid last component.
pub fn get_project_name_from_path<'a>(path: &'a Path, default: &'a str) -> &'a str {
	path.file_name().and_then(|name| name.to_str()).unwrap_or(default)
}

/// A macro to facilitate the select multiple variant of an enum and store them inside a `Vec`.
/// # Arguments
/// * `$enum`: The enum type to be iterated over for the selection. This enum must implement
///   `IntoEnumIterator` and `EnumMessage` traits from the `strum` crate. Each variant is
///   responsible of its own messages.
/// * `$prompt_message`: The message displayed to the user. It must implement the `Display` trait.
/// # Note
/// This macro only works with a 1-byte sized enums, this is, fieldless enums with at most 255
/// elements each. This is because we're just interested in letting the user to pick some options
/// among a predefined set, then the name should be descriptive enough, and 1-byte sized enums are
/// really easy to convert to and from a `u8`, so we can work with `u8` all the time and just
/// recover the variant at the end.
///
/// The decision of using 1-byte enums instead of just fieldless enums is for simplicity: we won't
/// probably offer a user to pick from > 256 options. If this macro is used with enums containing
/// fields, the conversion to `u8` will simply be detected at compile time and the compilation will
/// fail. If this macro is used with fieldless enums greater than 1-byte (really weird but
/// possible), the conversion to u8 will overflow and lead to unexpected behavior, so we panic at
/// runtime if that happens for completeness.
///
/// # Example
///
/// ```rust
/// use strum::{IntoEnumIterator, EnumMessage};
/// use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};
/// use cliclack::{multiselect};
/// use pop_common::multiselect_pick;
///
/// #[derive(Debug, EnumIter, EnumMessageDerive, Copy, Clone)]
/// enum FieldlessEnum {
///     #[strum(message = "Type 1", detailed_message = "Detailed message for Type 1")]
///     Type1,
///     #[strum(message = "Type 2", detailed_message = "Detailed message for Type 2")]
///     Type2,
///     #[strum(message = "Type 3", detailed_message = "Detailed message for Type 3")]
///     Type3,
/// }
///
/// fn test_function() -> Result<(),std::io::Error>{
///     let vec = multiselect_pick!(FieldlessEnum, "Hello, world!");
///     Ok(())
/// }
/// ```
///
/// # Requirements
///
/// This macro requires the following imports to function correctly:
///
/// ```rust
/// use cliclack::{multiselect};
/// use strum::{EnumMessage, IntoEnumIterator};
/// ```
///
/// Additionally, this macro handle results, so it must be used inside a function doing so.
/// Otherwise the compilation will fail.
#[macro_export]
macro_rules! multiselect_pick {
	($enum: ty, $prompt_message: expr) => {{
		// Ensure the enum is 1-byte long. This is needed cause fieldless enums with > 256 elements
		// will lead to unexpected behavior as the conversion to u8 for them isn't detected as wrong
		// at compile time. Enums containing variants with fields will be catched at compile time.
		// Weird but possible.
		assert!(std::mem::size_of::<$enum>() == 1);
		let mut prompt = multiselect(format!(
			"{} {}",
			$prompt_message,
			"Pick an option by pressing the spacebar. Press enter when you're done!"
		))
		.required(false);

		for variant in <$enum>::iter() {
			prompt = prompt.item(
				variant as u8,
				variant.get_message().unwrap_or_default(),
				variant.get_detailed_message().unwrap_or_default(),
			);
		}

		// The unsafe block is safe cause the bytes are the discriminants of the enum picked above,
		// qed;
		prompt
			.interact()?
			.iter()
			.map(|byte| unsafe { std::mem::transmute(*byte) })
			.collect::<Vec<$enum>>()
	}};
}

/// This function validates that a `&str` is a valid Rust identifier.
/// # Arguments
/// * `candidate` - A `&str` that may or may not be a valid Rust identifier
pub fn valid_ident(candidate: &str) -> anyhow::Result<()> {
	// The identifier cannot be a keyword
	let reserved_keywords = [
		"as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
		"for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
		"return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
		"use", "where", "while",
	];

	if reserved_keywords.contains(&candidate) {
		return Err(anyhow::anyhow!("A keyword cannot be used as identifier."));
	}

	let reg = RegexBuilder::new(r"^[a-z_][a-z0-9_]*$").case_insensitive(true).build()?;
	if reg.is_match(candidate) {
		Ok(())
	} else {
		Err(anyhow::anyhow!("Invalid identifier: {}.", candidate))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::fs;

	#[test]
	fn test_replace_in_file() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		let file_path = temp_dir.path().join("file.toml");
		let mut file = fs::File::create(temp_dir.path().join("file.toml"))?;
		writeln!(file, "name = test, version = 5.0.0")?;
		let mut replacements_in_cargo = HashMap::new();
		replacements_in_cargo.insert("test", "changed_name");
		replacements_in_cargo.insert("5.0.0", "5.0.1");
		replace_in_file(file_path.clone(), replacements_in_cargo)?;
		let content = fs::read_to_string(file_path).expect("Could not read file");
		assert_eq!(content.trim(), "name = changed_name, version = 5.0.1");
		Ok(())
	}

	#[test]
	fn get_project_name_from_path_works() -> Result<(), Error> {
		let path = Path::new("./path/to/project/my-parachain");
		assert_eq!(get_project_name_from_path(path, "default_name"), "my-parachain");
		Ok(())
	}

	#[test]
	fn get_project_name_from_path_default_value() -> Result<(), Error> {
		let path = Path::new("./");
		assert_eq!(get_project_name_from_path(path, "my-contract"), "my-contract");
		Ok(())
	}

	#[test]
	fn valid_ident_works_well() {
		let input = "hello";
		assert!(valid_ident(input).is_ok());
	}

	#[test]
	fn valid_ident_fails_with_keyword() {
		let input = "where";
		assert!(valid_ident(input).is_err());
	}

	#[test]
	fn valid_ident_fails_with_bad_input() {
		let input = "2hello";
		assert!(valid_ident(input).is_err());
	}
}
