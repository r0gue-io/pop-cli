// SPDX-License-Identifier: GPL-3.0

//! Parses and identifies the latest version tags based on semantic or Polkadot SDK versioning.

use crate::SortedSlice;
use regex::Regex;
use std::{cmp::Reverse, sync::LazyLock};

// Regex for `polkadot-stableYYMM` and `polkadot-stableYYMM-X`
static STABLE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r"(polkadot-(parachain-)?)?stable(?P<year>\d{2})(?P<month>\d{2})(-(?P<patch>\d+))?(-rc\d+)?",
	)
	.expect("Valid regex")
});
// Regex for v{major}.{minor}.{patch} format
static VERSION: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"v(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)(-rc\d+)?").expect("Valid regex")
});

/// A tuple of version numbers.
pub type Version = (u32, u32, u32);

/// Identifies the latest tag from a list of tags, prioritizing those in a `stableYYMM-X` format.
/// Prerelease versions are omitted.
///
/// # Arguments
/// * `tags` - A vector of tags to parse and evaluate.
pub fn parse_latest_tag(tags: &[impl AsRef<str>]) -> Option<&str> {
	match parse_latest_stable_tag(tags) {
		Some(last_stable_tag) => Some(last_stable_tag),
		None => parse_latest_semantic_version(tags),
	}
}

/// Identifies the latest `stableYYMM-X` release tag. Prerelease versions are omitted.
fn parse_latest_stable_tag(tags: &[impl AsRef<str>]) -> Option<&str> {
	tags.iter()
		.filter_map(|tag| parse_stable_version(tag.as_ref()).map(|version| (tag, version)))
		.max_by(|a, b| {
			let (_, (year_a, month_a, patch_a)) = a;
			let (_, (year_b, month_b, patch_b)) = b;
			// Compare by year, then by month, then by patch number
			year_a
				.cmp(year_b)
				.then_with(|| month_a.cmp(month_b))
				.then_with(|| patch_a.cmp(patch_b))
		})
		.map(|(tag, _)| tag.as_ref())
}

/// Identifies the latest version based on semantic versioning - e.g. `v1.2.3-rc`. Prerelease
/// versions are omitted.
///
/// # Arguments
/// * `items` - A vector of items to parse and evaluate.
pub fn parse_latest_semantic_version(items: &[impl AsRef<str>]) -> Option<&str> {
	items
		.iter()
		.filter_map(|tag| parse_semantic_version(tag.as_ref()).map(|version| (tag, version)))
		.max_by_key(|&(_, version)| version)
		.map(|(tag, _)| tag.as_ref())
}

/// Parses a semantic version - e.g. `v1.2.3-rc`. Prerelease versions are omitted.
///
/// # Arguments
/// * `value` - The value to parse and evaluate.
pub fn parse_semantic_version(value: impl AsRef<str>) -> Option<Version> {
	// Skip the pre-release label
	let value = value.as_ref();
	if value.contains("-rc") {
		return None;
	}
	VERSION.captures(value).and_then(|v| {
		let major = v.name("major")?.as_str().parse::<u32>().ok()?;
		let minor = v.name("minor")?.as_str().parse::<u32>().ok()?;
		let patch = v.name("patch")?.as_str().parse::<u32>().ok()?;
		Some((major, minor, patch))
	})
}

/// Parses a stable version - e.g. `stable2512-1`. Prerelease versions are omitted.
///
/// # Arguments
/// * `value` - The value to parse and evaluate.
pub fn parse_stable_version(value: &str) -> Option<Version> {
	// Skip the pre-release label
	if value.contains("-rc") {
		return None;
	}
	STABLE.captures(value).and_then(|v| {
		let year = v.name("year")?.as_str().parse::<u32>().ok()?;
		let month = v.name("month")?.as_str().parse::<u32>().ok()?;
		let patch = v.name("patch").and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
		Some((year, month, patch))
	})
}

/// Parses a version - e.g. `v1.2.3-rc` or `stableYYMM-X`, prioritizing those in a `stableYYMM-X`
/// format. Prerelease versions are omitted.
///
/// # Arguments
/// * `value` - The value to parse and evaluate.
pub fn parse_version(value: &str) -> Option<Version> {
	match parse_stable_version(value) {
		Some(stable_version) => Some(stable_version),
		None => parse_semantic_version(value),
	}
}

/// Sorts the provided versions using stable and semantic versioning,
/// with the latest version first. Prerelease versions are omitted.
///
/// # Arguments
/// * `versions` - The versions to sort.
pub fn sort_by_latest_semantic_version<T: AsRef<str>>(versions: &mut [T]) -> SortedSlice<'_, T> {
	SortedSlice::by_key(versions, |tag| {
		parse_version(tag.as_ref())
			.map(|version| Reverse(Some(version)))
			.unwrap_or(Reverse(None))
	})
}

/// Sorts the provided versions using `stableYYMM-X` versioning, with the latest version first.
/// Prerelease versions are omitted.
///
/// # Arguments
/// * `versions` - The versions to sort.
pub fn sort_by_latest_stable_version<T: AsRef<str>>(versions: &mut [T]) -> SortedSlice<'_, T> {
	SortedSlice::by_key(versions, |tag| {
		parse_stable_version(tag.as_ref())
			.map(|version| Reverse(Some(version)))
			.unwrap_or(Reverse(None))
	})
}

/// Sorts the provided versions using `stableYYMM-X` and semver versioning, with the latest version
/// first. Prerelease versions are omitted.
///
/// # Arguments
/// * `versions` - The versions to sort.
pub fn sort_by_latest_version<T: AsRef<str>>(versions: &mut [T]) -> SortedSlice<'_, T> {
	SortedSlice::by_key(versions, |tag| {
		parse_version(tag.as_ref())
			.map(|version| Reverse(Some(version)))
			.unwrap_or(Reverse(None))
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_latest_tag_works() {
		let mut tags = vec![];
		assert_eq!(parse_latest_tag(&tags), None);
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
		assert_eq!(parse_latest_tag(&tags), Some("polkadot-stable2409-1"));
	}

	#[test]
	fn parse_stable_format_works() {
		let mut tags = vec![];
		assert_eq!(parse_latest_stable_tag(&tags), None);
		tags = vec!["polkadot-stable2407", "polkadot-stable2408"];
		assert_eq!(parse_latest_stable_tag(&tags), Some("polkadot-stable2408"));
		tags = vec!["polkadot-stable2407", "polkadot-stable2501"];
		assert_eq!(parse_latest_stable_tag(&tags), Some("polkadot-stable2501"));
		// Skip the pre-release label
		tags = vec!["polkadot-stable2407", "polkadot-stable2407-1", "polkadot-stable2407-1-rc1"];
		assert_eq!(parse_latest_stable_tag(&tags), Some("polkadot-stable2407-1"));
	}

	#[test]
	fn parse_latest_semantic_version_works() {
		let mut tags: Vec<&str> = vec![];
		assert_eq!(parse_latest_semantic_version(&tags), None);
		tags = vec![
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
		];
		assert_eq!(parse_latest_semantic_version(&tags), Some("polkadot-v1.12.0"));
		tags = vec!["v1.0.0", "v2.0.0", "v3.0.0"];
		assert_eq!(parse_latest_semantic_version(&tags), Some("v3.0.0"));
		// Skip the pre-release label
		tags = vec!["polkadot-v1.12.0", "v1.15.1-rc2"];
		assert_eq!(parse_latest_semantic_version(&tags), Some("polkadot-v1.12.0"));
	}

	#[test]
	fn parse_version_works() {
		for (tag, expected) in [
			("polkadot-stable2409", Some((24, 9, 0))),
			("polkadot-stable2409-1", Some((24, 9, 1))),
			("polkadot-v1.18.0", Some((1, 18, 0))),
			("polkadot-v1.18.1", Some((1, 18, 1))),
			("v1.15.1", Some((1, 15, 1))),
			("v1.15.1-rc2", None),
		] {
			assert_eq!(parse_version(tag), expected);
		}
	}

	#[test]
	fn sort_by_latest_semantic_version_works() {
		assert_eq!(
			sort_by_latest_semantic_version(
				[
					"polkadot-v1.10.0",
					"polkadot-v1.11.0",
					"v1.17.0",
					"polkadot-v1.12.0",
					"polkadot-v1.7.0",
					"v1.18.0",
					"polkadot-v1.8.0",
					"polkadot-v1.9.0",
					"v1.18.1",
					"stable2612",
					"stable2512-7",
					"stable2407-5",
				]
				.as_mut_slice()
			)
			.0,
			[
				"stable2612",
				"stable2512-7",
				"stable2407-5",
				"v1.18.1",
				"v1.18.0",
				"v1.17.0",
				"polkadot-v1.12.0",
				"polkadot-v1.11.0",
				"polkadot-v1.10.0",
				"polkadot-v1.9.0",
				"polkadot-v1.8.0",
				"polkadot-v1.7.0",
			]
		);
	}

	#[test]
	fn sort_by_latest_stable_version_works() {
		assert_eq!(
			sort_by_latest_stable_version(
				[
					"polkadot-stable2409",
					"polkadot-stable2409-1",
					"polkadot-stable2407",
					"polkadot-stable2612",
					"polkadot-stable2512-1"
				]
				.as_mut_slice()
			)
			.0,
			[
				"polkadot-stable2612",
				"polkadot-stable2512-1",
				"polkadot-stable2409-1",
				"polkadot-stable2409",
				"polkadot-stable2407",
			]
		);
	}

	#[test]
	fn sort_by_latest_version_works() {
		assert_eq!(
			sort_by_latest_version(
				[
					"polkadot-v1.10.0",
					"polkadot-v1.11.0",
					"v1.17.0",
					"polkadot-v1.12.0",
					"polkadot-v1.7.0",
					"v1.18.0",
					"polkadot-v1.8.0",
					"polkadot-v1.9.0",
					"v1.18.1",
					"polkadot-stable2409",
					"polkadot-stable2409-1",
					"polkadot-stable2407",
					"polkadot-stable2612",
					"polkadot-stable2512-1"
				]
				.as_mut_slice()
			)
			.0,
			[
				"polkadot-stable2612",
				"polkadot-stable2512-1",
				"polkadot-stable2409-1",
				"polkadot-stable2409",
				"polkadot-stable2407",
				"v1.18.1",
				"v1.18.0",
				"v1.17.0",
				"polkadot-v1.12.0",
				"polkadot-v1.11.0",
				"polkadot-v1.10.0",
				"polkadot-v1.9.0",
				"polkadot-v1.8.0",
				"polkadot-v1.7.0",
			]
		);
	}
}
