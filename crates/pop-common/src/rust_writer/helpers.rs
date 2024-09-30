use crate::Error;
use regex::{Captures, Regex};
use syn::{parse_file, File};

pub(crate) fn preserve_and_parse(code: String) -> Result<File, Error> {
	let mut preserved_code = String::new();
	code.lines().for_each(|line| {
		let trimmed_line = line.trim_start();
		if trimmed_line.starts_with("//") &&
			!trimmed_line.starts_with("///") &&
			!trimmed_line.starts_with("//!")
		{
			// Use trimmed_line to avoid tabs before TEMP_DOC
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
			preserved_code.push_str(&format!("{}\n", line));
		}
	});

	Ok(parse_file(&preserved_code)?)
}

pub(crate) fn resolve_preserved(code: String) -> String {
	let mut output = String::new();
	// Inside declarative macros invocations, everything is a token so the comments became #[doc].
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
			comment if trimmed_line.strip_prefix("///TEMP_DOC").is_some() =>
				output.push_str(comment
                    .strip_prefix("///TEMP_DOC")
                    .expect("The match guard guarantees this is always some; qed;")
				),
			_ if trimmed_line.strip_prefix("///EMPTY_LINE").is_some() => output.push('\n'),
			_ => output.push_str(&format!("{}\n", line)),
		}
	});
	output
}

pub(crate) fn capitalize_str(input: &str) -> String {
	if input.is_empty() {
		return String::new();
	}

	let first_char = input.chars().next().expect("The introduced str isn't empty").to_uppercase();
	let rest = &input[1..];
	format!("{}{}", first_char, rest)
}
