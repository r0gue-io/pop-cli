// SPDX-License-Identifier: GPL-3.0

use crate::{style::style, cli::traits::*};
use anyhow::Result;
use clap::{Args, Subcommand};
use std::{
	fs::{read_dir, remove_file},
	path::PathBuf,
};
use cliclack::{confirm, outro_cancel};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CleanArgs {
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Remove generated/cached artifacts.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Remove cached artifacts.
	#[clap(alias = "c")]
	Cache(CleanCommandArgs),
}

#[derive(Args)]
pub struct CleanCommandArgs {
	#[arg(short = 'a', long)]
	all: bool
}

/// Removes cached artifacts.
pub(crate) struct CleanCacheCommand<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// The cache to be used.
	pub(crate) cache: PathBuf,
}

impl<'a, CLI: Cli> CleanCacheCommand<'a, CLI> {
	/// Executes the command.
	pub(crate) fn execute(self, args: CleanCommandArgs) -> Result<()> {
		self.cli.intro("Remove cached artifacts")?;

		// Get the cache contents
		if !self.cache.exists() {
			self.cli.outro_cancel("🚫 The cache does not exist.")?;
			return Ok(());
		};
		let contents = contents(&self.cache)?;
		if contents.is_empty() {
			self.cli.outro(format!(
				"ℹ️ The cache at {} is empty.",
				self.cache.to_str().expect("expected local cache is invalid")
			))?;
			return Ok(());
		}
		self.cli.info(format!(
			"ℹ️ The cache is located at {}",
			self.cache.to_str().expect("expected local cache is invalid")
		))?;

		// if args.all flag is present, then remove all artifacts
		if args.all {
			// Display all artifacts to be deleted and get confirmation
			let list = style(format!(
				"> {}",
				&contents
					.iter()
					.map(|(name, _, size)| format!("{} : {}MiB", name, size / 1_048_576))
					.collect::<Vec<_>>()
					.join(", ")
			))
			.dim()
			.to_string();

			if !confirm(format!(
				"Would you like to cleanup all cache artifacts...\n {list}"))
			.initial_value(true)
			.interact()? {
				outro_cancel("Failed to clean cache")?;
				return Ok(());
			}

			for (_, file, _) in &contents {
				// confirm removal
				remove_file(&file)?;
				self.cli.outro(format!("ℹ️ {} artifacts removed", contents.len()))?;
			}
		} else {
			// Prompt for selection of artifacts to be removed
			let selected = {
				let mut prompt =
					self.cli.multiselect("Select the artifacts you wish to remove:").required(false);
				for (name, path, size) in &contents {
					prompt = prompt.item(path, name, format!("{}MiB", size / 1_048_576))
				}
				prompt.interact()?
			};
			if selected.is_empty() {
				self.cli.outro("ℹ️ No artifacts removed")?;
				return Ok(());
			};

			// Confirm removal
			let prompt = match selected.len() {
				1 => "Are you sure you want to remove the selected artifact?".into(),
				_ => format!(
					"Are you sure you want to remove the {} selected artifacts?",
					selected.len()
				),
			};
			if !self.cli.confirm(prompt).interact()? {
				self.cli.outro("ℹ️ No artifacts removed")?;
				return Ok(());
			}

			// Finally remove selected artifacts
			for file in &selected {
				remove_file(file)?
			}

			self.cli.outro(format!("ℹ️ {} artifacts removed", selected.len()))?;
		}

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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::fs::File;

	#[test]
	fn clean_cache_has_intro() -> Result<()> {
		let cache = PathBuf::new();
		let mut cli = MockCli::new().expect_intro(&"Remove cached artifacts");
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		cli.verify()
	}

	#[test]
	fn clean_cache_handles_missing_cache() -> Result<()> {
		let cache = PathBuf::new();
		let mut cli = MockCli::new().expect_outro_cancel(&"🚫 The cache does not exist.");
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		cli.verify()
	}

	#[test]
	fn clean_cache_handles_empty_cache() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let mut cli = MockCli::new()
			.expect_outro(&format!("ℹ️ The cache at {} is empty.", cache.to_str().unwrap()));
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		cli.verify()
	}

	#[test]
	fn clean_cache_outputs_cache_location() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		for artifact in ["polkadot"] {
			File::create(cache.join(artifact))?;
		}
		let mut cli = MockCli::new()
			.expect_info(format!("ℹ️ The cache is located at {}", cache.to_str().unwrap()));
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		cli.verify()
	}

	#[test]
	fn clean_cache_prompts_for_selection() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let mut items = vec![];
		for artifact in ["polkadot", "pop-node"] {
			File::create(cache.join(artifact))?;
			items.push((artifact.to_string(), "0MiB".to_string()))
		}
		let mut cli = MockCli::new().expect_multiselect::<PathBuf>(
			"Select the artifacts you wish to remove:",
			Some(false),
			true,
			Some(items),
		);
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		cli.verify()
	}

	#[test]
	fn clean_cache_removes_nothing_when_no_selection() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let artifacts = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"]
			.map(|a| cache.join(a));
		for artifact in &artifacts {
			File::create(artifact)?;
		}
		let mut cli = MockCli::new()
			.expect_multiselect::<PathBuf>(
				"Select the artifacts you wish to remove:",
				Some(false),
				false,
				None,
			)
			.expect_outro("ℹ️ No artifacts removed");
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		for artifact in artifacts {
			assert!(artifact.exists())
		}
		cli.verify()
	}

	#[test]
	fn clean_cache_confirms_removal() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let artifacts = ["polkadot-parachain"];
		for artifact in artifacts {
			File::create(cache.join(artifact))?;
		}
		let mut cli = MockCli::new()
			.expect_multiselect::<PathBuf>(
				"Select the artifacts you wish to remove:",
				None,
				true,
				None,
			)
			.expect_confirm("Are you sure you want to remove the selected artifact?", false)
			.expect_outro("ℹ️ No artifacts removed");
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		cli.verify()
	}

	#[test]
	fn clean_cache_removes_selection() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let artifacts = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"]
			.map(|a| cache.join(a));
		for artifact in &artifacts {
			File::create(artifact)?;
		}
		let mut cli = MockCli::new()
			.expect_multiselect::<PathBuf>(
				"Select the artifacts you wish to remove:",
				None,
				true,
				None,
			)
			.expect_confirm("Are you sure you want to remove the 3 selected artifacts?", true)
			.expect_outro("ℹ️ 3 artifacts removed");
		let args = CleanCommandArgs {
			all: false,
		};
		CleanCacheCommand { cli: &mut cli, cache }.execute(args)?;

		for artifact in artifacts {
			assert!(!artifact.exists())
		}
		cli.verify()
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
}
