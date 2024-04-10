use anyhow::Result;
use git2::{IndexAddOption, Repository, ResetType};
use regex::Regex;
use std::{fs, path::Path};
/// Clone `url` into `target` and degit it
pub(crate) fn clone_and_degit(url: &str, target: &Path) -> Result<Option<String>> {
	let repo = Repository::clone(url, target)?;

	// fetch tags from remote
	let release = fetch_latest_tag(&repo);

	let git_dir = repo.path();
	fs::remove_dir_all(&git_dir)?;
	Ok(release)
}

/// Fetch the latest release from a repository
fn fetch_latest_tag(repo: &Repository) -> Option<String> {
	let version_reg = Regex::new(r"v\d+\.\d+\.\d+").expect("Valid regex");
	let tags = repo.tag_names(None).ok()?;
	// Start from latest tags
	for tag in tags.iter().rev() {
		if let Some(tag) = tag {
			if version_reg.is_match(tag) {
				return Some(tag.to_string());
			}
		}
	}
	None
}

/// Init a new git repo on creation of a parachain
pub fn git_init(target: &Path, message: &str) -> Result<(), git2::Error> {
	let repo = Repository::init(target)?;
	let signature = repo.signature()?;

	let mut index = repo.index()?;
	index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
	let tree_id = index.write_tree()?;

	let tree = repo.find_tree(tree_id)?;
	let commit_id = repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?;

	let commit_object = repo.find_object(commit_id, Some(git2::ObjectType::Commit))?;
	repo.reset(&commit_object, ResetType::Hard, None)?;

	Ok(())
}
