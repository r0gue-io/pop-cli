use crate::Error;
use regex::{Captures, Regex};
use syn::{parse_file, File};

pub(crate) fn preserve_and_parse(code: String, exempt_macros: Vec<&str>) -> Result<File, Error> {
	// First of all, preserve declarative macros except those that are exempted. As declarative
	// macros invocations AST are basically a TokenStream, they don't keep format when unparsed and
	// rustfmt doesn't work well at all inside all macro invocations, so it's better to keep them
	// commented unless we explicitly need to modify them
	let code = preserve_macro_invocations(code, exempt_macros);
	let mut preserved_code = String::new();
	// Preserve the rest of the code
	code.lines().for_each(|line| {
		let trimmed_line = line.trim_start();
		if trimmed_line.starts_with("//") &&
			!trimmed_line.starts_with("///") &&
			!trimmed_line.starts_with("//!")
		{
			preserved_code
				.push_str(&format!("///TEMP_DOC{}\ntype temp_marker = ();\n", trimmed_line));
		} else if trimmed_line.starts_with("#![") {
			// Global attributes may be hard to parse with syn, so we comment them to solve
			// potential issues related to them.
			preserved_code
				.push_str(&format!("///TEMP_DOC{}\ntype temp_marker = ();\n", trimmed_line));
		} else if trimmed_line.is_empty() {
			preserved_code.push_str("///EMPTY_LINE\ntype temp_marker = ();\n");
		} else {
			preserved_code.push_str(&format!("{}\n", trimmed_line));
		}
	});
	Ok(parse_file(&preserved_code)?)
}

pub(crate) fn resolve_preserved(code: String) -> String {
	let mut output = String::new();
	// Inside non-preserved declarative macros invocations, everything is a token so the doc
	// comments became #[doc] in order to preserve them (tokens doesn't accept doc comments).
	// ///EMPTY_LINE comments became #[doc = "EMPTY_LINE"] which are 4 tokens in the AST. When the
	// AST is converted to a String, new line characters can appear in the middle of any of those
	// tokens, so to properly convert them in a new line we can use regex.
	let mut re =
		Regex::new(r#"#\s*\[\s*doc\s*=\s*"EMPTY_LINE"\s*\]"#).expect("The regex is valid; qed;");
	let code = re.replace_all(&code, "\n").to_string();
	// Same happens with 'type temp_marker = ();'. This lines also delete them from everywhere, not
	// just inside declarative macros
	re = Regex::new(r"type\s+temp_marker\s*=\s*\(\);\s*").expect("The regex is valid; qed;");
	let code = re.replace_all(&code, "\n").to_string();
	// Same happens with #[doc= "TEMP_DOC whatever"] but we also need to keep track of "whatever".
	// As the #[doc] attribute may be present anywhere, be sure to keep spaces before and after the
	// comment to don't leave commented some lines of code.
	re = Regex::new(r#"#\s*\[\s*doc\s*=\s*"TEMP_DOC(.*?)"\s*\]"#).expect("The regex is valid;qed;");
	let code = re.replace_all(&code, |caps: &Captures| format!("\n{}\n", &caps[1])).to_string();

	// Resolve the comments outside declarative macros.
	code.lines().for_each(|line| {
		let trimmed_line = line.trim_start();

		match trimmed_line {
			comment if trimmed_line.strip_prefix("///TEMP_DOC").is_some() => output.push_str(
				comment
					.strip_prefix("///TEMP_DOC")
					.expect("The match guard guarantees this is always some; qed;"),
			),
			comment if trimmed_line.strip_prefix("///TEMP_MACRO").is_some() =>
				output.push_str(&format!(
					"{}\n",
					comment
						.strip_prefix("///TEMP_MACRO")
						.expect("The match guard guarantees this is always some; qed;"),
				)),
			_ if trimmed_line.strip_prefix("///EMPTY_LINE").is_some() => output.push('\n'),
			_ => output.push_str(&format!("{}\n", line)),
		}
	});
	output
}

fn preserve_macro_invocations(code: String, exempt_macros: Vec<&str>) -> String {
	let re =
		Regex::new(r"(?P<macro_name>[a-zA-Z_]+)!\s*[\{\(\[]").expect("The regex is valid; qed;");
	let mut result = String::new();

	let mut macro_block = String::new();

	let mut delimiter_counts = [('{', 0), ('}', 0), ('(', 0), (')', 0), ('[', 0), (']', 0)];

	for line in code.lines() {
		match re.find(line) {
			Some(found)
				if exempt_macros.contains(
					&&re.captures(found.as_str()).expect(
						"As found has been found with the regex, it captures the macro_name; qed;",
					)["macro_name"],
				) =>
				result.push_str(&format!("{}\n", line)),
			Some(_) => {
				macro_block.push_str(&format!("{}\n", line));
				for (char, count) in delimiter_counts.iter_mut() {
					if line.contains(*char) {
						*count += line.matches(*char).count();
					}
				}
			},
			None if macro_block.is_empty() => result.push_str(&format!("{}\n", line)),
			_ => {
				macro_block.push_str(&format!("{}\n", line));
				for (char, count) in delimiter_counts.iter_mut() {
					if line.contains(*char) {
						*count += line.matches(*char).count();
					}
				}
			},
		}
		if delimiter_counts[0].1 == delimiter_counts[1].1 &&
			delimiter_counts[2].1 == delimiter_counts[3].1 &&
			delimiter_counts[4].1 == delimiter_counts[5].1 &&
			!macro_block.is_empty()
		{
			macro_block.lines().for_each(|line| {
				result.push_str(&format!("///TEMP_MACRO{}\n", line));
			});
			result.push_str("type temp_marker = ();\n");
			macro_block = String::new();
		}
	}
	result
}
