// SPDX-License-Identifier: GPL-3.0

use crate::{cli::traits::*, style::style};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde::Serialize;
use std::{
	fs::{read_dir, remove_file},
	path::PathBuf,
	process::Command as StdCommand,
};

#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CleanArgs {
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Remove generated/cached artifacts.
#[derive(Subcommand, Serialize)]
pub(crate) enum Command {
	/// Remove all processes running.
	#[clap(alias = "n")]
	Node(CleanCommandArgs),
	/// Remove cached artifacts.
	#[clap(alias = "c")]
	Cache(CleanCommandArgs),
}

#[derive(Args, Serialize)]
pub struct CleanCommandArgs {
	/// Pass flag to remove all cache artifacts or running nodes.
	#[arg(short, long)]
	pub(crate) all: bool,
}

/// The result of a clean command.
#[derive(Serialize)]
pub struct CleanData {
	/// A human-readable message.
	pub message: String,
	/// The number of items (artifacts or processes) removed.
	pub count: usize,
}

/// Removes cached artifacts.
pub(crate) struct CleanCacheCommand<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// The cache to be used.
	pub(crate) cache: PathBuf,
	/// Whether to clean all artifacts.
	pub(crate) all: bool,
}

impl<CLI: Cli> CleanCacheCommand<'_, CLI> {
	/// Executes the command.
	pub(crate) fn execute(self) -> Result<serde_json::Value> {
		self.cli.intro("Remove cached artifacts")?;

		// Get the cache contents
		if !self.cache.exists() {
			let msg = "🚫 The cache does not exist.";
			self.cli.outro_cancel(msg)?;
			return Err(anyhow::anyhow!(msg));
		};
		let contents = contents(&self.cache)?;
		if contents.is_empty() {
			let msg = format!(
				"ℹ️ The cache at {} is empty.",
				self.cache.to_str().expect("expected local cache is invalid")
			);
			self.cli.outro(&msg)?;
			return Ok(serde_json::to_value(CleanData { message: msg, count: 0 })?);
		}
		self.cli.info(format!(
			"ℹ️ The cache is located at {}",
			self.cache.to_str().expect("expected local cache is invalid")
		))?;

		let mut count = 0;
		if self.all {
			// Display all artifacts to be deleted and get confirmation
			let list = style(format!(
				"\n{}",
				&contents
					.iter()
					.map(|(name, _, size)| format!("{} : {}MiB", name, size / 1_048_576))
					.collect::<Vec<_>>()
					.join("; \n")
			))
			.to_string();

			self.cli.info(format!("Cleaning up the following artifacts...\n {list} \n"))?;
			for (_, file, _) in &contents {
				// confirm removal
				remove_file(file)?;
				count += 1;
			}

			self.cli.outro(format!("ℹ️ {} artifacts removed", count))?;
		} else {
			// Prompt for selection of artifacts to be removed
			let selected = {
				let mut prompt = self
					.cli
					.multiselect("Select the artifacts you wish to remove:")
					.required(false);
				for (name, path, size) in &contents {
					prompt = prompt.item(path, name, format!("{}MiB", size / 1_048_576))
				}
				prompt.interact()?
			};
			if selected.is_empty() {
				let msg = "ℹ️ No artifacts removed";
				self.cli.outro(msg)?;
				return Ok(serde_json::to_value(CleanData { message: msg.to_string(), count: 0 })?);
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
				let msg = "ℹ️ No artifacts removed";
				self.cli.outro(msg)?;
				return Ok(serde_json::to_value(CleanData { message: msg.to_string(), count: 0 })?);
			}

			// Finally remove selected artifacts
			for file in &selected {
				remove_file(file)?;
				count += 1;
			}

			self.cli.outro(format!("ℹ️ {} artifacts removed", count))?;
		}

		Ok(serde_json::to_value(CleanData {
			message: format!("{} artifacts removed", count),
			count,
		})?)
	}
}

/// Returns the contents of the specified path.
fn contents(path: &PathBuf) -> Result<Vec<(String, PathBuf, u64)>> {
	let mut contents: Vec<_> = read_dir(path)?
		.filter_map(|e| {
			e.ok().and_then(|e| {
				e.file_name()
					.to_str()
					.map(|f| (f.to_string(), e.path()))
					.zip(e.metadata().ok())
					.map(|f| (f.0.0, f.0.1, f.1.len()))
			})
		})
		.filter(|(name, _, _)| !name.starts_with('.'))
		.collect();
	contents.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
	Ok(contents)
}

/// Kills running nodes.
pub(crate) struct CleanNodesCommand<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// Whether to clean all nodes.
	pub(crate) all: bool,
	/// Test hook: override process lister.
	#[cfg(test)]
	pub(crate) list_nodes: Option<Box<dyn Fn() -> Result<Vec<(String, String, String)>>>>,
	/// Test hook: override killer.
	#[cfg(test)]
	pub(crate) kill_fn: Option<Box<dyn Fn(&str) -> Result<()>>>,
}

impl<CLI: Cli> CleanNodesCommand<'_, CLI> {
	/// Executes the command.
	pub(crate) fn execute(self) -> Result<serde_json::Value> {
		self.cli.intro("Remove running nodes")?;

		// Get running processes for both ink-node and eth-rpc
		let processes = {
			#[cfg(test)]
			{
				if let Some(ref f) = self.list_nodes { f()? } else { get_node_processes()? }
			}
			#[cfg(not(test))]
			{
				get_node_processes()?
			}
		};

		if processes.is_empty() {
			let msg = "ℹ️ No running nodes found.";
			self.cli.outro(msg)?;
			return Ok(serde_json::to_value(CleanData { message: msg.to_string(), count: 0 })?);
		}

		let mut count = 0;
		if self.all {
			// Display all processes to be killed
			let list = style(format!(
				"\n{}",
				&processes
					.iter()
					.map(|(name, pid, ports)| format!("{} (PID {}) : ports {}", name, pid, ports))
					.collect::<Vec<_>>()
					.join("; \n")
			))
			.to_string();

			self.cli.info(format!("Killing the following processes...\n {list} \n"))?;

			for (_, pid, _) in &processes {
				#[cfg(test)]
				{
					if let Some(ref f) = self.kill_fn { f(pid)? } else { kill_process(pid)? }
				}
				#[cfg(not(test))]
				{
					kill_process(pid)?
				}
				count += 1;
			}

			self.cli.outro(format!("ℹ️ {} processes killed", count))?;
		} else {
			// Prompt for selection of processes to be killed
			let selected = {
				let mut prompt =
					self.cli.multiselect("Select the processes you wish to kill:").required(false);
				for (name, pid, ports) in &processes {
					prompt = prompt.item(
						pid,
						format!("{} (PID {})", name, pid),
						format!("ports: {}", ports),
					)
				}
				prompt.interact()?
			};

			if selected.is_empty() {
				let msg = "ℹ️ No processes killed";
				self.cli.outro(msg)?;
				return Ok(serde_json::to_value(CleanData { message: msg.to_string(), count: 0 })?);
			}

			// Confirm removal
			let prompt = match selected.len() {
				1 => "Are you sure you want to kill the selected process?".into(),
				_ => format!(
					"Are you sure you want to kill the {} selected processes?",
					selected.len()
				),
			};
			if !self.cli.confirm(prompt).interact()? {
				let msg = "ℹ️ No processes killed";
				self.cli.outro(msg)?;
				return Ok(serde_json::to_value(CleanData { message: msg.to_string(), count: 0 })?);
			}

			// Finally kill selected processes
			for pid in &selected {
				#[cfg(test)]
				{
					if let Some(ref f) = self.kill_fn { f(pid)? } else { kill_process(pid)? }
				}
				#[cfg(not(test))]
				{
					kill_process(pid)?
				}
				count += 1;
			}

			self.cli.outro(format!("ℹ️ {} processes killed", count))?;
		}

		Ok(serde_json::to_value(CleanData {
			message: format!("{} processes killed", count),
			count,
		})?)
	}
}

/// Returns a list of (process_name, PID, ports) for ink-node and eth-rpc processes.
fn get_node_processes() -> Result<Vec<(String, String, String)>> {
	let mut processes = Vec::new();

	// Process types to check
	let process_names = ["ink-node", "eth-rpc"];

	for process_name in &process_names {
		// Get PIDs using pgrep
		let pgrep_output = StdCommand::new("pgrep").arg(process_name).output()?;

		if !pgrep_output.status.success() {
			continue;
		}

		let pids = String::from_utf8_lossy(&pgrep_output.stdout);

		for pid in pids.lines().filter(|l| !l.is_empty()) {
			// Get ports for this PID using lsof
			let lsof_output = StdCommand::new("lsof")
				.args(["-Pan", "-p", pid, "-i", "TCP", "-s", "TCP:LISTEN"])
				.output()?;

			if !lsof_output.status.success() {
				continue;
			}

			let lsof_lines = String::from_utf8_lossy(&lsof_output.stdout);
			let mut ports = Vec::new();

			for line in lsof_lines.lines().skip(1) {
				if line.contains("127.0.0.1") {
					let parts: Vec<&str> = line.split_whitespace().collect();
					if let Some(addr) = parts.get(8) &&
						let Some(port) = addr.split(':').next_back()
					{
						ports.push(port.to_string());
					}
				}
			}

			if !ports.is_empty() {
				processes.push((process_name.to_string(), pid.to_string(), ports.join(", ")));
			}
		}
	}

	Ok(processes)
}

/// Kills a process by PID.
fn kill_process(pid: &str) -> Result<()> {
	StdCommand::new("kill").arg("-9").arg(pid).output()?;
	Ok(())
}

#[cfg(test)]
impl Default for CleanArgs {
	fn default() -> Self {
		Self { command: Command::Cache(CleanCommandArgs { all: false }) }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::fs::File;

	#[test]
	fn clean_cache_has_intro() -> Result<()> {
		let cache = PathBuf::new();
		let mut cli = MockCli::new().expect_intro("Remove cached artifacts");

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

		cli.verify()
	}

	#[test]
	fn clean_cache_handles_missing_cache() -> Result<()> {
		let cache = PathBuf::new();
		let mut cli = MockCli::new().expect_outro_cancel("🚫 The cache does not exist.");

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

		cli.verify()
	}

	#[test]
	fn clean_cache_handles_empty_cache() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let mut cli = MockCli::new()
			.expect_outro(format!("ℹ️ The cache at {} is empty.", cache.to_str().unwrap()));

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

		cli.verify()
	}

	#[test]
	fn clean_cache_outputs_cache_location() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		{
			let artifact = "polkadot";
			File::create(cache.join(artifact))?;
		}
		let mut cli = MockCli::new()
			.expect_info(format!("ℹ️ The cache is located at {}", cache.to_str().unwrap()));

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

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
		let mut cli = MockCli::new().expect_multiselect(
			"Select the artifacts you wish to remove:",
			Some(false),
			true,
			Some(items),
			None,
		);

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

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
			.expect_multiselect(
				"Select the artifacts you wish to remove:",
				Some(false),
				false,
				None,
				None,
			)
			.expect_outro("ℹ️ No artifacts removed");

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

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
			.expect_multiselect("Select the artifacts you wish to remove:", None, true, None, None)
			.expect_confirm("Are you sure you want to remove the selected artifact?", false)
			.expect_outro("ℹ️ No artifacts removed");

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

		cli.verify()
	}

	#[test]
	fn clean_cache_cleans_dir_when_all_flag_specified() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let cache = temp.path().to_path_buf();
		let artifacts = ["polkadot-parachain", "pop-node"];
		let mut items = vec![];
		for artifact in &artifacts {
			File::create(cache.join(artifact))?;
			items.push((artifact, "0MiB".to_string()));
		}

		let list = style(format!(
			"\n{}",
			items
				.iter()
				.map(|(name, size)| format!("{} : {}", name, size))
				.collect::<Vec<_>>()
				.join("; \n")
		))
		.to_string();

		let mut cli = MockCli::new()
			.expect_info(format!("Cleaning up the following artifacts...\n {list} \n"))
			.expect_outro("ℹ️ 2 artifacts removed");

		CleanCacheCommand { cli: &mut cli, cache, all: true }.execute()?;

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
			.expect_multiselect("Select the artifacts you wish to remove:", None, true, None, None)
			.expect_confirm("Are you sure you want to remove the 3 selected artifacts?", true)
			.expect_outro("ℹ️ 3 artifacts removed");

		CleanCacheCommand { cli: &mut cli, cache, all: false }.execute()?;

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

	#[test]
	fn clean_nodes_handles_no_processes() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_outro("ℹ️ No running nodes found.");

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			#[cfg(test)]
			list_nodes: Some(Box::new(|| Ok(vec![]))),
			#[cfg(test)]
			kill_fn: None,
		};

		cmd.execute()?;

		cli.verify()
	}

	#[test]
	fn clean_nodes_all_kills_processes() -> Result<()> {
		use std::{cell::RefCell, rc::Rc};
		let processes = vec![
			("ink-node".to_string(), "111".to_string(), "30333, 9944".to_string()),
			("eth-rpc".to_string(), "222".to_string(), "8545".to_string()),
		];

		let list = style(format!(
			"\n{}",
			&processes
				.iter()
				.map(|(name, pid, ports)| format!("{} (PID {}) : ports {}", name, pid, ports))
				.collect::<Vec<_>>()
				.join("; \n")
		))
		.to_string();

		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_info(format!("Killing the following processes...\n {list} \n"))
			.expect_outro("ℹ️ 2 processes killed");

		let killed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let killed2 = killed.clone();
		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: true,
			list_nodes: Some(Box::new(move || Ok(processes.clone()))),
			kill_fn: Some(Box::new(move |pid: &str| {
				killed2.borrow_mut().push(pid.to_string());
				Ok(())
			})),
		};

		cmd.execute()?;
		assert_eq!(&*killed.borrow(), &vec!["111".to_string(), "222".to_string()]);
		cli.verify()
	}

	#[test]
	fn clean_nodes_multiselect_no_selection() -> Result<()> {
		let processes = vec![("ink-node".to_string(), "111".to_string(), "30333".to_string())];
		let items = vec![("ink-node (PID 111)".to_string(), "ports: 30333".to_string())];
		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_multiselect(
				"Select the processes you wish to kill:",
				Some(false),
				false,
				Some(items),
				None,
			)
			.expect_outro("ℹ️ No processes killed");

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			list_nodes: Some(Box::new(move || Ok(processes.clone()))),
			kill_fn: Some(Box::new(|_| unreachable!("kill should not be called"))),
		};

		cmd.execute()?;
		cli.verify()
	}

	#[test]
	fn clean_nodes_multiselect_confirm_false() -> Result<()> {
		let processes = vec![("ink-node".to_string(), "111".to_string(), "30333".to_string())];
		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_multiselect("Select the processes you wish to kill:", None, true, None, None)
			.expect_confirm("Are you sure you want to kill the selected process?", false)
			.expect_outro("ℹ️ No processes killed");

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			list_nodes: Some(Box::new(move || Ok(processes.clone()))),
			kill_fn: Some(Box::new(|_| unreachable!("kill should not be called"))),
		};

		cmd.execute()?;
		cli.verify()
	}

	#[test]
	fn clean_nodes_multiselect_confirm_true() -> Result<()> {
		use std::{cell::RefCell, rc::Rc};
		let processes = vec![
			("ink-node".to_string(), "111".to_string(), "30333".to_string()),
			("eth-rpc".to_string(), "222".to_string(), "8545".to_string()),
			("ink-node".to_string(), "333".to_string(), "30334".to_string()),
		];
		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_multiselect("Select the processes you wish to kill:", None, true, None, None)
			.expect_confirm("Are you sure you want to kill the 3 selected processes?", true)
			.expect_outro("ℹ️ 3 processes killed");

		let killed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let killed2 = killed.clone();

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			list_nodes: Some(Box::new(move || Ok(processes.clone()))),
			kill_fn: Some(Box::new(move |pid: &str| {
				killed2.borrow_mut().push(pid.to_string());
				Ok(())
			})),
		};

		cmd.execute()?;
		assert_eq!(
			&*killed.borrow(),
			&["111", "222", "333"].iter().map(|s| s.to_string()).collect::<Vec<_>>()
		);
		cli.verify()
	}
}
