// SPDX-License-Identifier: GPL-3.0

//! Parses and identifies the latest version tags for Polkadot SDK releases.

use regex::Regex;

/// Identifies the latest tag from a list of tags, prioritizing tags in a stable format.
///
/// # Arguments
/// * `tags` - A vector of tags to parse and evaluate.
pub fn parse_latest_tag(tags: Vec<&str>) -> Option<String> {
	match parse_latest_stable(&tags) {
		Some(last_stable_tag) => Some(last_stable_tag),
		None => parse_version_format(&tags),
	}
}

/// Retrieves the latest stable release tag.
fn parse_latest_stable(tags: &[&str]) -> Option<String> {
	// Regex for polkadot-stableYYMM and polkadot-stableYYMM-X
	let stable_reg = Regex::new(
		r"(polkadot-(parachain-)?)?stable(?P<year>\d{2})(?P<month>\d{2})(-(?P<patch>\d+))?(-rc\d+)?",
	)
	.expect("Valid regex");
	tags.iter()
		.filter_map(|tag| {
			// Skip the pre-release label
			if tag.contains("-rc") {
				return None;
			}
			stable_reg.captures(tag).and_then(|v| {
				let year = v.name("year")?.as_str().parse::<u32>().ok()?;
				let month = v.name("month")?.as_str().parse::<u32>().ok()?;
				let patch =
					v.name("patch").and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
				Some((tag, (year, month, patch)))
			})
		})
		.max_by(|a, b| {
			let (_, (year_a, month_a, patch_a)) = a;
			let (_, (year_b, month_b, patch_b)) = b;
			// Compare by year, then by month, then by patch number
			year_a
				.cmp(year_b)
				.then_with(|| month_a.cmp(month_b))
				.then_with(|| patch_a.cmp(patch_b))
		})
		.map(|(tag_str, _)| tag_str.to_string())
}

/// Parse the versioning release tags.
fn parse_version_format(tags: &[&str]) -> Option<String> {
	// Regex for polkadot-vmajor.minor.patch format
	let version_reg = Regex::new(r"v(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)(-rc\d+)?")
		.expect("Valid regex");
	tags.iter()
		.filter_map(|tag| {
			// Skip the pre-release label
			if tag.contains("-rc") {
				return None;
			}
			version_reg.captures(tag).and_then(|v| {
				let major = v.name("major")?.as_str().parse::<u32>().ok()?;
				let minor = v.name("minor")?.as_str().parse::<u32>().ok()?;
				let patch = v.name("patch")?.as_str().parse::<u32>().ok()?;
				Some((tag, (major, minor, patch)))
			})
		})
		.max_by_key(|&(_, version)| version)
		.map(|(tag_str, _)| tag_str.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_latest_tag_works() {
		let mut tags = vec![];
		assert_eq!(parse_latest_tag(tags), None);
		tags = vec![
			"polkadot-stable2409",
			"polkadot-stable2409-1",
			"polkadot-stable2407",
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
			"v1.15.1-rc2",
		];
		assert_eq!(parse_latest_tag(tags), Some("polkadot-stable2409-1".to_string()));
	}

	#[test]
	fn parse_stable_format_works() {
		let mut tags = vec![];
		assert_eq!(parse_latest_stable(&tags), None);
		tags = vec!["polkadot-stable2407", "polkadot-stable2408"];
		assert_eq!(parse_latest_stable(&tags), Some("polkadot-stable2408".to_string()));
		tags = vec!["polkadot-stable2407", "polkadot-stable2501"];
		assert_eq!(parse_latest_stable(&tags), Some("polkadot-stable2501".to_string()));
		// Skip the pre-release label
		tags = vec!["polkadot-stable2407", "polkadot-stable2407-1", "polkadot-stable2407-1-rc1"];
		assert_eq!(parse_latest_stable(&tags), Some("polkadot-stable2407-1".to_string()));
	}

	#[test]
	fn parse_version_format_works() {
		let mut tags: Vec<&str> = vec![];
		assert_eq!(parse_version_format(&tags), None);
		tags = vec![
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
		];
		assert_eq!(parse_version_format(&tags), Some("polkadot-v1.12.0".to_string()));
		tags = vec!["v1.0.0", "v2.0.0", "v3.0.0"];
		assert_eq!(parse_version_format(&tags), Some("v3.0.0".to_string()));
		// Skip the pre-release label
		tags = vec!["polkadot-v1.12.0", "v1.15.1-rc2"];
		assert_eq!(parse_version_format(&tags), Some("polkadot-v1.12.0".to_string()));
	}
}
