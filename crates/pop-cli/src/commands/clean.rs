// SPDX-License-Identifier: GPL-3.0

use crate::{cache, style::Theme};
use anyhow::Result;
use clap::{Args, Subcommand};
use cliclack::{clear_screen, confirm, intro, log, multiselect, outro, outro_cancel, set_theme};
use console::style;
use std::{
	fs::{read_dir, remove_file},
	path::PathBuf,
};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CleanArgs {
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Remove generated/cached artifacts.
#[derive(Subcommand)]
pub(crate) enum Command {
	#[cfg(feature = "parachain")]
	/// Remove cached artifacts.
	#[clap(alias = "c")]
	Cache(CleanCacheCommand),
}

#[derive(Args)]
pub(crate) struct CleanCacheCommand;

impl CleanCacheCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> Result<()> {
		clear_screen()?;
		set_theme(Theme);
		intro(format!("{}: Remove cached artifacts", style(" Pop CLI ").black().on_magenta()))?;

		// Get the cache contents
		let cache = cache()?;
		if !cache.exists() {
			outro_cancel("🚫 The cache does not exist.")?;
			return Ok(());
		};
		let contents = contents(&cache)?;
		if contents.is_empty() {
			outro(format!(
				"ℹ️ The cache at {} is empty.",
				cache.to_str().expect("expected local cache is invalid")
			))?;
			return Ok(());
		}
		log::info(format!(
			"ℹ️ The cache is located at {}",
			cache.to_str().expect("expected local cache is invalid")
		))?;

		// Prompt for selection of artifacts to be removed
		let mut prompt = multiselect("Select the artifacts you wish to remove:").required(false);
		for (name, path, size) in &contents {
			prompt = prompt.item(path, name, format!("{}MiB", size / 1_048_576))
		}
		let selected = prompt.interact()?;
		if selected.is_empty() {
			outro("ℹ️ No artifacts removed")?;
			return Ok(());
		}

		// Confirm removal
		let prompt = match selected.len() {
			1 => "Are you sure you want to remove the selected artifact?".into(),
			_ => format!(
				"Are you sure you want to remove the {} selected artifacts?",
				selected.len()
			),
		};
		if !confirm(prompt).interact()? {
			outro("ℹ️ No artifacts removed")?;
			return Ok(());
		}

		// Finally remove selected artifacts
		for file in &selected {
			remove_file(file)?
		}
		outro(format!("ℹ️ {} artifacts removed", selected.len()))?;
		Ok(())
	}
}

/// Returns the contents of the specified path.
fn contents(path: &PathBuf) -> Result<Vec<(String, PathBuf, u64)>> {
	let mut contents: Vec<_> = read_dir(&path)?
		.filter_map(|e| {
			e.ok().and_then(|e| {
				e.file_name()
					.to_str()
					.map(|f| (f.to_string(), e.path()))
					.zip(e.metadata().ok())
					.map(|f| (f.0 .0, f.0 .1, f.1.len()))
			})
		})
		.filter(|(name, _, _)| !name.starts_with("."))
		.collect();
	contents.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
	Ok(contents)
}

#[test]
fn contents_works() -> Result<()> {
	use std::fs::File;
	let temp = tempfile::tempdir()?;
	let cache = temp.path().to_path_buf();
	let mut files = vec!["a", "z", "1"];
	for file in &files {
		File::create(cache.join(file))?;
	}
	files.sort();

	let contents = contents(&cache)?;
	assert_eq!(
		contents,
		files.iter().map(|f| (f.to_string(), cache.join(f), 0)).collect::<Vec<_>>()
	);
	Ok(())
}
