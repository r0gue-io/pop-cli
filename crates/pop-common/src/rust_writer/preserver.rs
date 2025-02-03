// SPDX-License-Identifier: GPL-3.0

use crate::{
	rust_writer::types::{DelimitersCount, Preserver},
	Error,
};
use regex::{Captures, Regex};
use syn::{parse_file, File};

#[cfg(test)]
mod tests;

pub(crate) fn preserve_and_parse(code: String, preservers: Vec<Preserver>) -> Result<File, Error> {
	let preserved_code = apply_preservers(code, preservers);
	Ok(parse_file(&preserved_code)?)
}

pub(crate) fn resolve_preserved(code: String) -> String {
	// Inside non-preserved declarative macros invocations, everything is a token and hence it
	// should be managed carefully. We capture all the macro invocations and apply regex to those
	// pieces of code to properly resolve them.
	let mut delimiters_counts = DelimitersCount::new();
	let mut lines = code.lines();

	// We'll reduce lines, so this capacity is a max bond on the result
	let mut macro_cleaned_code: Vec<String> = Vec::with_capacity(code.lines().count());
	let mut macro_content = String::new();

	let macro_invocation_matcher =
		Regex::new(r"\w+!\s*[\{\(\[]").expect("The regex is valid; qed;");

	// Inside declarative macros, doc comments became #[doc] in order to preserve them (tokens
	// doesn't accept doc comments). ///TEMP_DOC comments became #[doc = "TEMP_DOC(something)"]
	// which are 4 tokens in the AST. When the AST is converted to a String, new line characters
	// can appear in the middle of any of those tokens, so to properly convert them in a new line
	// we can use regex. As the #[doc] attribute may be present anywhere, be sure to keep spaces
	// before and after the comment to don't leave commented some lines of code.
	let macro_docs_matcher = Regex::new(r#"#\s*\[\s*doc\s*=\s*"TEMP_DOC([\\t]*)(.*?)"\s*\]"#)
		.expect("The regex is valid; qed;");
	// Same happens with 'type temp_marker = ();'.
	let temp_marker_matcher =
		Regex::new(r"type\s+temp_marker\s*=\s*\(\);\s*").expect("The regex is valid; qed;");

	while let Some(line) = lines.next() {
		// We're noting the content of a macro
		if !macro_content.is_empty() && !delimiters_counts.is_complete() {
			delimiters_counts.count(line);
			macro_content.push_str(line);
			macro_content.push_str("\n");
			// Start noting the content of a macro
		} else if macro_invocation_matcher.is_match(&line) {
			delimiters_counts.count(line);
			macro_content.push_str(line);
			macro_content.push_str("\n");
			// macro_content contains the whole macro, so we preserve it and push it, together with
			// the new line to the cleaned code
		} else if delimiters_counts.is_complete() {
			let docs_resolved_code = macro_docs_matcher
				.replace_all(&macro_content, |caps: &Captures| format!("\n{}\n", &caps[2]))
				.to_string();

			macro_cleaned_code
				.push(temp_marker_matcher.replace_all(&docs_resolved_code, "\n").to_string());
			macro_cleaned_code.push(line.to_owned());
			macro_cleaned_code.push("\n".to_owned());

			macro_content.clear();
		} else {
			macro_cleaned_code.push(line.to_owned());
			macro_cleaned_code.push("\n".to_owned());
		}
	}

	// Delete all TEMP_DOCS and temp_marker present in the rest of the code and return the result.
	macro_cleaned_code
		.join("")
		.replace("///TEMP_DOC", "")
		.replace("type temp_marker = ();\n", "")
}

fn apply_preservers(code: String, mut preservers: Vec<Preserver>) -> String {
	let mut delimiters_counts = DelimitersCount::new();

	let mut lines = code.lines();

	// Non-preserved lines are pushed to the Vec together with a new line character, so the bound
	// #lines * 2 is an upper bound of the final capacity
	let mut result: Vec<String> = Vec::with_capacity(code.lines().count() * 2);

	while let Some(line) = lines.next() {
		let trimmed_line = line.trim_start();
		if let Some(index) = preservers
			.iter_mut()
			.position(|preserver| trimmed_line.starts_with(preserver.lookup()))
		{
			delimiters_counts.count(line);
			result.push(line.to_owned());
			result.push("\n".to_owned());

			let mut preserver = preservers.swap_remove(index);
			let inner_preserver = preserver.take_inner();

			if let Some(inner_preserver_pointer) = inner_preserver {
				let mut inner_code = String::new();
				while let Some(line) = lines.next() {
					delimiters_counts.count(line);

					if delimiters_counts.is_complete() {
						result.push(apply_preservers(inner_code, vec![*inner_preserver_pointer]));
						result.push(line.to_owned());
						result.push("\n".to_owned());
						break;
					} else {
						inner_code.push_str(line);
						inner_code.push_str("\n");
					}
				}
			}
		} else {
			if delimiters_counts.is_complete() {
				result.push(format!("///TEMP_DOC{}\n", line));
			} else {
				if (trimmed_line.starts_with("//") &&
					!trimmed_line.starts_with("///") &&
					!trimmed_line.starts_with("//!")) ||
					trimmed_line.starts_with("#![")
				{
					// Preserve comments and global attributes.
					// Global attributes may be hard to parse with syn, so we comment them to solve
					// potential issues related to them.
					result.push(format!("///TEMP_DOC{}\ntype temp_marker = ();", line));
				} else if trimmed_line.is_empty() {
					// Preserve empty lines inside a non-preserved block
					result.push("///TEMP_DOC\ntype temp_marker = ();".to_owned());
				} else {
					result.push(line.to_owned());
					result.push("\n".to_owned());
				}

				delimiters_counts.count(line);
			}
		}
	}

	result.push("type temp_marker = ();".to_owned());

	result.join("")
}
