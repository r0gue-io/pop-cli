// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	output::{CliResponse, OutputMode, PromptRequiredError},
	style::style,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde::Serialize;
use serde_json::Value;
use std::{
	fs::{read_dir, remove_dir_all, remove_file},
	io::IsTerminal,
	path::{Path, PathBuf},
	process::Command as StdCommand,
};
use time::format_description::well_known::Rfc3339;

const INFO: &str = "ℹ️  ";

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
	/// Remove running network(s).
	#[clap(alias = "net")]
	Network(CleanNetworkCommandArgs),
}

#[derive(Args, Serialize)]
pub struct CleanCommandArgs {
	/// Pass flag to remove all cache artifacts or running nodes.
	#[arg(short, long)]
	pub(crate) all: bool,
	/// Pass one or more process IDs to remove artifacts for specific processes.
	#[arg(short, long, num_args = 1..)]
	pub(crate) pid: Option<Vec<String>>,
}

#[derive(Args, Serialize)]
pub struct CleanNetworkCommandArgs {
	/// Stop all running networks without prompting.
	#[arg(short, long)]
	pub(crate) all: bool,
	/// Path to the network base directory or zombie.json.
	#[arg(value_name = "PATH")]
	pub(crate) path: Option<PathBuf>,
	/// Keep the network state on disk after shutdown (default: remove state).
	#[arg(long)]
	pub(crate) keep_state: bool,
}

#[cfg(feature = "chain")]
async fn destroy_network(zombie_json: &Path) -> Result<()> {
	pop_chains::up::destroy_network(zombie_json).await.map_err(Into::into)
}

#[cfg(not(feature = "chain"))]
async fn destroy_network(_zombie_json: &Path) -> Result<()> {
	anyhow::bail!("network cleanup requires the `chain` feature")
}

/// Structured output for `clean cache --json`.
#[derive(Serialize)]
struct CleanCacheOutput {
	removed: Vec<CleanCacheItem>,
}

#[derive(Serialize)]
struct CleanCacheItem {
	name: String,
	size_bytes: u64,
}

/// Structured output for `clean node --json`.
#[derive(Serialize)]
struct CleanNodeOutput {
	killed: Vec<CleanNodeItem>,
}

#[derive(Serialize)]
struct CleanNodeItem {
	name: String,
	pid: String,
	ports: String,
}

/// Structured output for `clean network --json`.
#[derive(Serialize)]
struct CleanNetworkOutput {
	stopped: Vec<String>,
	failures: usize,
}

/// Entry point called from the command dispatcher.
pub(crate) async fn execute(
	args: &CleanArgs,
	cache_fn: impl FnOnce() -> Result<PathBuf>,
	output_mode: OutputMode,
) -> Result<()> {
	match output_mode {
		OutputMode::Human => match &args.command {
			Command::Cache(cmd_args) => CleanCacheCommand {
				cli: &mut crate::cli::Cli,
				cache: cache_fn()?,
				all: cmd_args.all,
			}
			.execute(),
			Command::Node(cmd_args) => CleanNodesCommand {
				cli: &mut crate::cli::Cli,
				all: cmd_args.all,
				pid: cmd_args.pid.clone(),
				#[cfg(test)]
				list_nodes: None,
				#[cfg(test)]
				kill_fn: None,
			}
			.execute(),
			Command::Network(cmd_args) =>
				CleanNetworkCommand {
					cli: &mut crate::cli::Cli,
					all: cmd_args.all,
					path: cmd_args.path.clone(),
					keep_state: cmd_args.keep_state,
				}
				.execute()
				.await,
		},
		OutputMode::Json => match &args.command {
			Command::Cache(cmd_args) => execute_cache_json(cache_fn()?, cmd_args),
			Command::Node(cmd_args) => execute_node_json(cmd_args),
			Command::Network(cmd_args) => execute_network_json(cmd_args).await,
		},
	}
}

fn execute_cache_json(cache: PathBuf, args: &CleanCommandArgs) -> Result<()> {
	if !args.all {
		return Err(PromptRequiredError("--all is required with --json".into()).into());
	}
	if !cache.exists() {
		CliResponse::ok(CleanCacheOutput { removed: vec![] }).print_json();
		return Ok(());
	}
	let items = contents(&cache)?;
	if items.is_empty() {
		CliResponse::ok(CleanCacheOutput { removed: vec![] }).print_json();
		return Ok(());
	}
	let mut removed = Vec::new();
	for (name, file, size) in &items {
		remove_file(file)?;
		removed.push(CleanCacheItem { name: name.clone(), size_bytes: *size });
	}
	CliResponse::ok(CleanCacheOutput { removed }).print_json();
	Ok(())
}

fn execute_node_json(args: &CleanCommandArgs) -> Result<()> {
	if !args.all && args.pid.is_none() {
		return Err(PromptRequiredError("--all or --pid is required with --json".into()).into());
	}
	let processes = get_node_processes()?;
	if processes.is_empty() {
		CliResponse::ok(CleanNodeOutput { killed: vec![] }).print_json();
		return Ok(());
	}
	let pids_to_kill: Vec<String> = if args.all {
		processes.iter().map(|(_, pid, _)| pid.clone()).collect()
	} else if let Some(pids) = &args.pid {
		let valid_pids: Vec<&str> = processes.iter().map(|(_, pid, _)| pid.as_str()).collect();
		let invalid: Vec<&String> =
			pids.iter().filter(|pid| !valid_pids.contains(&pid.as_str())).collect();
		if !invalid.is_empty() {
			anyhow::bail!(
				"Invalid PID(s): {}",
				invalid.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")
			);
		}
		pids.clone()
	} else {
		vec![]
	};

	let mut killed = Vec::new();
	for pid in &pids_to_kill {
		let info = processes.iter().find(|(_, p, _)| p == pid);
		kill_process(pid)?;
		if let Some((name, _, ports)) = info {
			killed.push(CleanNodeItem {
				name: name.clone(),
				pid: pid.clone(),
				ports: ports.clone(),
			});
		}
	}
	CliResponse::ok(CleanNodeOutput { killed }).print_json();
	Ok(())
}

async fn execute_network_json(args: &CleanNetworkCommandArgs) -> Result<()> {
	if !args.all && args.path.is_none() {
		return Err(PromptRequiredError("--all or PATH is required with --json".into()).into());
	}
	let zombie_jsons = if args.all {
		let candidates = find_zombie_jsons()?;
		if candidates.is_empty() {
			CliResponse::ok(CleanNetworkOutput { stopped: vec![], failures: 0 }).print_json();
			return Ok(());
		}
		candidates.into_iter().map(|c| c.path).collect()
	} else {
		vec![resolve_zombie_json_path(args.path.as_ref().unwrap())?]
	};

	let mut stopped = Vec::new();
	let mut failures = 0;
	for zombie_json in &zombie_jsons {
		let base_dir = zombie_json
			.parent()
			.ok_or_else(|| anyhow::anyhow!("invalid zombie.json path"))?
			.to_path_buf();
		if let Err(e) = destroy_network(zombie_json).await {
			let error_message = e.to_string();
			if should_fallback_to_process_cleanup(&error_message) {
				let pids = read_pids_from_zombie_json(zombie_json)?;
				for pid in pids {
					kill_process(&pid)?;
				}
			} else if !is_already_stopped_error(&error_message) {
				failures += 1;
				continue;
			}
		}

		if args.keep_state {
			let _ = mark_cleared(&base_dir);
		} else if remove_dir_all(&base_dir).is_err() {
			failures += 1;
			continue;
		}
		stopped.push(base_dir.display().to_string());
	}
	CliResponse::ok(CleanNetworkOutput { stopped, failures }).print_json();
	Ok(())
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
	pub(crate) fn execute(self) -> Result<()> {
		self.cli.intro("Remove cached artifacts")?;

		// Get the cache contents
		if !self.cache.exists() {
			self.cli.outro_cancel("🚫 The cache does not exist.")?;
			return Ok(());
		};
		let contents = contents(&self.cache)?;
		if contents.is_empty() {
			self.cli.outro(format!(
				"{INFO}The cache at {} is empty.",
				self.cache.to_str().expect("expected local cache is invalid")
			))?;
			return Ok(());
		}
		self.cli.info(format!(
			"{INFO}The cache is located at {}",
			self.cache.to_str().expect("expected local cache is invalid")
		))?;

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
			}

			self.cli.outro(format!("{INFO}{} artifacts removed", contents.len()))?;
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
				self.cli.outro(format!("{INFO}No artifacts removed"))?;
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
				self.cli.outro(format!("{INFO}No artifacts removed"))?;
				return Ok(());
			}

			// Finally remove selected artifacts
			for file in &selected {
				remove_file(file)?
			}

			self.cli.outro(format!("{INFO}{} artifacts removed", selected.len()))?;
		}

		Ok(())
	}
}

/// Stops a running network and optionally removes its state.
pub(crate) struct CleanNetworkCommand<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// Whether to stop all running networks without prompting.
	pub(crate) all: bool,
	/// Path to the network base directory or zombie.json.
	pub(crate) path: Option<PathBuf>,
	/// Whether to keep the network state on disk.
	pub(crate) keep_state: bool,
}

impl<CLI: Cli> CleanNetworkCommand<'_, CLI> {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<()> {
		self.cli.intro("Remove running network")?;

		let path_provided = self.path.is_some();
		let (zombie_jsons, selection_kind) = if self.all {
			let candidates = find_zombie_jsons()?;
			if candidates.is_empty() {
				self.cli.outro(format!("{INFO}No running networks found."))?;
				return Ok(());
			}
			(candidates.into_iter().map(|c| c.path).collect(), NetworkSelection::Selected)
		} else {
			match self.path.as_ref() {
				Some(path) => (vec![resolve_zombie_json_path(path)?], NetworkSelection::Specified),
				None => {
					let candidates = find_zombie_jsons()?;
					if candidates.is_empty() {
						self.cli.outro(format!("{INFO}No running networks found."))?;
						return Ok(());
					}
					if candidates.len() == 1 {
						(vec![candidates[0].path.clone()], NetworkSelection::AutoSingle)
					} else {
						let selection = {
							let mut prompt = self
								.cli
								.multiselect("Select the networks to stop (type to filter):")
								.required(false)
								.filter_mode();
							for candidate in &candidates {
								let base_dir = candidate.path.parent().unwrap_or(&candidate.path);
								let label = base_dir
									.file_name()
									.and_then(|f| f.to_str())
									.unwrap_or("network");
								let hint = format!(
									"modified: {}",
									candidate
										.modified
										.map(|t| t
											.format(&Rfc3339)
											.unwrap_or_else(|_| "unknown".into()))
										.unwrap_or_else(|| "unknown".into())
								);
								prompt = prompt.item(candidate.path.clone(), label, hint);
							}
							prompt.interact()?
						};
						if selection.is_empty() {
							self.cli.outro(format!("{INFO}No networks stopped."))?;
							return Ok(());
						}
						(selection, NetworkSelection::Selected)
					}
				},
			}
		};

		let count = zombie_jsons.len();
		let single_location = if count == 1 {
			zombie_jsons[0]
				.parent()
				.map(|p| p.display().to_string())
				.unwrap_or_else(|| zombie_jsons[0].display().to_string())
		} else {
			String::new()
		};
		let confirm = match (count, selection_kind) {
			(1, NetworkSelection::AutoSingle) => format!("Stop the network at {single_location}?"),
			(1, NetworkSelection::Specified) => {
				format!("Stop the specified network at {single_location}?")
			},
			(1, NetworkSelection::Selected) => "Stop the selected network?".to_string(),
			_ => format!("Stop the {} selected networks?", count),
		};
		if !self.all &&
			!path_provided &&
			std::io::stdin().is_terminal() &&
			!self.cli.confirm(confirm).initial_value(true).interact()?
		{
			self.cli.outro(format!("{INFO}No networks stopped."))?;
			return Ok(());
		}

		let mut failures = 0;
		for zombie_json in &zombie_jsons {
			let base_dir = zombie_json
				.parent()
				.ok_or_else(|| anyhow::anyhow!("invalid zombie.json path"))?
				.to_path_buf();
			if let Err(e) = destroy_network(zombie_json).await {
				let error_message = e.to_string();
				if should_fallback_to_process_cleanup(&error_message) {
					self.cli.warning(format!(
						"⚠️  Falling back to process cleanup for attached network at {}",
						base_dir.display()
					))?;
					let pids = read_pids_from_zombie_json(zombie_json)?;
					if pids.is_empty() {
						self.cli.warning(format!(
							"{INFO}Network already stopped at {}",
							base_dir.display()
						))?;
					} else {
						for pid in pids {
							kill_process(&pid)?;
						}
					}
				} else if is_already_stopped_error(&error_message) {
					self.cli.warning(format!(
						"{INFO}Network already stopped at {}",
						base_dir.display()
					))?;
				} else {
					failures += 1;
					self.cli.warning(format!(
						"🚫  Failed to stop network at {}: {e}",
						base_dir.display()
					))?;
					continue;
				}
			}

			if self.keep_state {
				if let Err(e) = mark_cleared(&base_dir) {
					self.cli.warning(format!(
						"🚫  Failed to mark network as cleared at {}: {e}",
						base_dir.display()
					))?;
				}
				self.cli
					.info(format!("{INFO}Network stopped. State kept at {}", base_dir.display()))?;
				continue;
			}

			if let Err(e) = remove_dir_all(&base_dir) {
				self.cli.warning(format!(
					"🚫  Failed to remove network state at {}: {e}",
					base_dir.display()
				))?;
				failures += 1;
			}
		}

		if failures > 0 {
			self.cli.warning(format!(
				"⚠️  Completed with {} failure{}.",
				failures,
				if failures == 1 { "" } else { "s" }
			))?;
		} else if self.keep_state {
			self.cli.outro(format!("{INFO}Networks stopped. State kept."))?;
		} else {
			self.cli.outro(format!("{INFO}Networks stopped and state removed"))?;
		}
		Ok(())
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

fn resolve_zombie_json_path(path: &Path) -> Result<PathBuf> {
	if path.is_file() {
		if path.file_name().and_then(|f| f.to_str()) == Some("zombie.json") {
			return Ok(path.to_path_buf());
		}
		anyhow::bail!("Expected a zombie.json file at {}", path.display());
	}

	if path.is_dir() {
		let candidate = path.join("zombie.json");
		if candidate.exists() {
			return Ok(candidate);
		}
		anyhow::bail!("No zombie.json found in {}", path.display());
	}

	anyhow::bail!("Invalid path: {}", path.display())
}

fn mark_cleared(base_dir: &Path) -> Result<()> {
	let cleared = base_dir.join(".CLEARED");
	std::fs::write(cleared, "")?;
	Ok(())
}

fn should_fallback_to_process_cleanup(message: &str) -> bool {
	message.contains("Cannot abort a recreated/attached node") ||
		message.contains("not connected") ||
		message.contains("Provider error")
}

fn is_already_stopped_error(message: &str) -> bool {
	message.contains("ProcessIdRetrievalFailed") ||
		message.contains("no process was attached") ||
		message.contains("ESRCH") ||
		message.contains("No such process")
}

fn read_pids_from_zombie_json(path: &Path) -> Result<Vec<String>> {
	let contents = std::fs::read_to_string(path)?;
	let value: Value = serde_json::from_str(&contents)?;
	let mut pids = Vec::new();
	collect_pids(&value, &mut pids);
	pids.sort();
	pids.dedup();
	Ok(pids)
}

fn collect_pids(value: &Value, pids: &mut Vec<String>) {
	match value {
		Value::Object(map) =>
			for (key, value) in map {
				if key == "pid" || key == "process" {
					match value {
						Value::Number(number) => push_pid(number.to_string(), pids),
						Value::String(pid) => push_pid(pid.to_string(), pids),
						_ => {},
					}
				}
				collect_pids(value, pids);
			},
		Value::Array(items) =>
			for item in items {
				collect_pids(item, pids);
			},
		_ => {},
	}
}

fn push_pid(pid: String, pids: &mut Vec<String>) {
	if pid.parse::<u32>().is_ok() {
		pids.push(pid);
	}
}

struct ZombieJsonCandidate {
	path: PathBuf,
	modified: Option<time::OffsetDateTime>,
}

fn find_zombie_jsons() -> Result<Vec<ZombieJsonCandidate>> {
	let temp_dir = std::env::temp_dir();
	let pattern = regex::Regex::new(
		r"^zombie-[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
	)
	.expect("expected valid regex");
	let mut candidates = Vec::new();
	for entry in read_dir(&temp_dir)? {
		let entry = match entry {
			Ok(entry) => entry,
			Err(_) => continue,
		};
		let path = entry.path();
		if !path.is_dir() {
			continue;
		}
		let name = entry.file_name();
		let name = match name.to_str() {
			Some(name) => name,
			None => continue,
		};
		if !pattern.is_match(name) {
			continue;
		}
		if path.join(".CLEARED").exists() {
			continue;
		}
		let zombie_json = path.join("zombie.json");
		if zombie_json.exists() {
			let modified = zombie_json
				.metadata()
				.and_then(|m| m.modified())
				.ok()
				.map(time::OffsetDateTime::from);
			candidates.push(ZombieJsonCandidate { path: zombie_json, modified });
		}
	}
	candidates.sort_by(|a, b| match (a.modified, b.modified) {
		(Some(a), Some(b)) => b.cmp(&a),
		(Some(_), None) => std::cmp::Ordering::Less,
		(None, Some(_)) => std::cmp::Ordering::Greater,
		(None, None) => b.path.cmp(&a.path),
	});
	Ok(candidates)
}

/// Kills running nodes.
pub(crate) struct CleanNodesCommand<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// Whether to clean all nodes.
	pub(crate) all: bool,
	/// PIDs to kill.
	pub(crate) pid: Option<Vec<String>>,
	/// Test hook: override process lister.
	#[cfg(test)]
	pub(crate) list_nodes: Option<Box<dyn Fn() -> Result<Vec<(String, String, String)>>>>,
	/// Test hook: override killer.
	#[cfg(test)]
	pub(crate) kill_fn: Option<Box<dyn Fn(&str) -> Result<()>>>,
}

impl<CLI: Cli> CleanNodesCommand<'_, CLI> {
	/// Executes the command.
	pub(crate) fn execute(self) -> Result<()> {
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
			self.cli.outro(format!("{INFO}No running nodes found."))?;
			return Ok(());
		}

		let pids = if self.all {
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
			processes.into_iter().map(|p| p.1.clone()).collect::<Vec<_>>()
		} else if let Some(pids) = &self.pid {
			// Validate that all provided PIDs exist in the running processes
			let valid_pids: Vec<&str> = processes.iter().map(|(_, pid, _)| pid.as_str()).collect();
			let invalid_pids: Vec<&String> =
				pids.iter().filter(|pid| !valid_pids.contains(&pid.as_str())).collect();

			if !invalid_pids.is_empty() {
				self.cli.outro_cancel(format!(
					"🚫  Invalid PID(s): {}. No processes killed.",
					invalid_pids.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")
				))?;
				return Ok(());
			}

			pids.clone()
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
				self.cli.outro(format!("{INFO}No processes killed"))?;
				return Ok(());
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
				self.cli.outro(format!("{INFO}No processes killed"))?;
				return Ok(());
			}

			selected.into_iter().cloned().collect::<Vec<_>>()
		};

		for pid in &pids {
			#[cfg(test)]
			{
				if let Some(ref f) = self.kill_fn { f(pid)? } else { kill_process(pid)? }
			}
			#[cfg(not(test))]
			{
				kill_process(pid)?
			}
		}

		self.cli.outro(format!("{INFO}{} processes killed", pids.len()))?;

		Ok(())
	}
}

#[derive(Clone, Copy)]
enum NetworkSelection {
	AutoSingle,
	Selected,
	Specified,
}

/// Returns a list of (process_name, PID, ports) for ink-node, eth-rpc, and pop-fork processes.
fn get_node_processes() -> Result<Vec<(String, String, String)>> {
	let mut processes = Vec::new();

	// Process types to check by exact name
	let process_names = ["ink-node", "eth-rpc"];

	for process_name in &process_names {
		// Get PIDs using pgrep
		let pgrep_output = StdCommand::new("pgrep").arg(process_name).output()?;

		if !pgrep_output.status.success() {
			continue;
		}

		let pids = String::from_utf8_lossy(&pgrep_output.stdout);

		for pid in pids.lines().filter(|l| !l.is_empty()) {
			// Get ports for this PID using lsof - propagate errors if lsof fails to execute
			let lsof_output = StdCommand::new("lsof")
				.args(["-Pan", "-p", pid, "-i", "TCP", "-s", "TCP:LISTEN"])
				.output()?;

			if !lsof_output.status.success() {
				continue;
			}

			if let Some(ports) = parse_lsof_ports(&lsof_output.stdout) {
				processes.push((process_name.to_string(), pid.to_string(), ports));
			}
		}
	}

	// Also check for pop-fork processes (identified by "fork --serve" in command line)
	processes.extend(get_fork_processes()?);

	Ok(processes)
}

/// Returns a list of (process_name, PID, ports) for detached pop-fork processes.
fn get_fork_processes() -> Result<Vec<(String, String, String)>> {
	let mut processes = Vec::new();

	// Find processes running "fork ... --serve" in command line.
	// Using [f]ork pattern to prevent pgrep from matching itself - the bracket notation
	// matches 'fork' in process args but not '[f]ork' in pgrep's own command line.
	let pgrep_output = StdCommand::new("pgrep").args(["-f", "[f]ork.*--serve"]).output()?;

	if !pgrep_output.status.success() {
		return Ok(processes);
	}

	let pids = String::from_utf8_lossy(&pgrep_output.stdout);

	for pid in pids.lines().filter(|l| !l.is_empty()) {
		// Get ports - propagate errors if lsof fails to execute
		let lsof_output = StdCommand::new("lsof")
			.args(["-Pan", "-p", pid, "-i", "TCP", "-s", "TCP:LISTEN"])
			.output()?;

		// Include process even if lsof returns non-success or no ports detected
		// (process might be starting up or not listening yet)
		let ports = if lsof_output.status.success() {
			parse_lsof_ports(&lsof_output.stdout).unwrap_or_else(|| "N/A".to_string())
		} else {
			"N/A".to_string()
		};
		processes.push(("pop-fork".to_string(), pid.to_string(), ports));
	}

	Ok(processes)
}

/// Parse lsof output to extract listening ports on 127.0.0.1.
fn parse_lsof_ports(output: &[u8]) -> Option<String> {
	let lsof_lines = String::from_utf8_lossy(output);
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

	if ports.is_empty() { None } else { Some(ports.join(", ")) }
}

/// Kills a process by PID.
fn kill_process(pid: &str) -> Result<()> {
	let output = StdCommand::new("kill").arg("-9").arg(pid).output()?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		let stdout = String::from_utf8_lossy(&output.stdout);
		let message = if stderr.is_empty() { stdout.to_string() } else { stderr.to_string() };
		if is_already_stopped_error(message.trim()) {
			return Ok(());
		}
		anyhow::bail!("failed to kill process {pid}: {message}");
	}
	Ok(())
}

#[cfg(test)]
impl Default for CleanArgs {
	fn default() -> Self {
		Self { command: Command::Cache(CleanCommandArgs { all: false, pid: None }) }
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
			.expect_outro(format!("{INFO}The cache at {} is empty.", cache.to_str().unwrap()));

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
			.expect_info(format!("{INFO}The cache is located at {}", cache.to_str().unwrap()));

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
			.expect_outro(format!("{INFO}No artifacts removed"));

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
			.expect_outro(format!("{INFO}No artifacts removed"));

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
			.expect_outro(format!("{INFO}2 artifacts removed"));

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
			.expect_outro(format!("{INFO}3 artifacts removed"));

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
	fn resolve_zombie_json_path_accepts_file() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let zombie_json = temp.path().join("zombie.json");
		std::fs::write(&zombie_json, "{}")?;

		let resolved = resolve_zombie_json_path(&zombie_json)?;
		assert_eq!(resolved, zombie_json);
		Ok(())
	}

	#[test]
	fn resolve_zombie_json_path_accepts_dir() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let zombie_json = temp.path().join("zombie.json");
		std::fs::write(&zombie_json, "{}")?;

		let resolved = resolve_zombie_json_path(temp.path())?;
		assert_eq!(resolved, zombie_json);
		Ok(())
	}

	#[test]
	fn resolve_zombie_json_path_rejects_missing() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let result = resolve_zombie_json_path(temp.path());
		assert!(result.is_err());
		Ok(())
	}

	#[test]
	fn read_pids_from_zombie_json_collects_pids() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let zombie_json = temp.path().join("zombie.json");
		std::fs::write(
			&zombie_json,
			r#"
			{
				"nodes": [
					{ "name": "alice", "pid": 1234 },
					{ "name": "bob", "pid": "5678" },
					{ "name": "charlie", "process": 9012 }
				],
				"other": { "nested": { "pid": 9999, "process": "3456" } }
			}
			"#,
		)?;

		let mut pids = read_pids_from_zombie_json(&zombie_json)?;
		pids.sort();
		assert_eq!(pids, vec!["1234", "3456", "5678", "9012", "9999"]);
		Ok(())
	}

	#[test]
	fn find_zombie_jsons_skips_cleared() -> Result<()> {
		let temp_dir = std::env::temp_dir().join("zombie-00000000-0000-0000-0000-000000000000");
		std::fs::create_dir_all(&temp_dir)?;
		std::fs::write(temp_dir.join("zombie.json"), "{}")?;
		std::fs::write(temp_dir.join(".CLEARED"), "")?;

		let candidates = find_zombie_jsons()?;
		assert!(!candidates.iter().any(|c| c.path.starts_with(&temp_dir)));
		std::fs::remove_dir_all(&temp_dir)?;
		Ok(())
	}

	#[test]
	fn already_stopped_error_detects_esrch() {
		let message = "Failed to kill attached process 1234: ESRCH: No such process";
		assert!(is_already_stopped_error(message));
	}

	#[test]
	fn fallback_cleanup_detects_provider_error() {
		let message = "Orchestrator error: Provider error";
		assert!(should_fallback_to_process_cleanup(message));
	}

	#[test]
	fn read_pids_from_zombie_json_ignores_non_numeric_process() -> Result<()> {
		let temp = tempfile::tempdir()?;
		let zombie_json = temp.path().join("zombie.json");
		std::fs::write(
			&zombie_json,
			r#"
			{
				"nodes": [
					{ "name": "alice", "pid": 1234 },
					{ "name": "bob", "process": "polkadot" },
					{ "name": "charlie", "process": "5678" }
				]
			}
			"#,
		)?;

		let mut pids = read_pids_from_zombie_json(&zombie_json)?;
		pids.sort();
		assert_eq!(pids, vec!["1234", "5678"]);
		Ok(())
	}

	#[test]
	fn clean_nodes_handles_no_processes() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_outro(format!("{INFO}No running nodes found."));

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			pid: None,
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
			.expect_outro(format!("{INFO}2 processes killed"));

		let killed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let killed2 = killed.clone();
		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: true,
			pid: None,
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
			.expect_outro(format!("{INFO}No processes killed"));

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			pid: None,
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
			.expect_outro(format!("{INFO}No processes killed"));

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			pid: None,
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
			.expect_outro(format!("{INFO}3 processes killed"));

		let killed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let killed2 = killed.clone();

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			pid: None,
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

	#[test]
	fn clean_nodes_pid_kills_specified_processes() -> Result<()> {
		use std::{cell::RefCell, rc::Rc};
		let processes = vec![
			("ink-node".to_string(), "111".to_string(), "30333".to_string()),
			("eth-rpc".to_string(), "222".to_string(), "8545".to_string()),
		];

		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_outro(format!("{INFO}1 processes killed"));

		let killed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let killed2 = killed.clone();

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			pid: Some(vec!["111".to_string()]),
			list_nodes: Some(Box::new(move || Ok(processes.clone()))),
			kill_fn: Some(Box::new(move |pid: &str| {
				killed2.borrow_mut().push(pid.to_string());
				Ok(())
			})),
		};

		cmd.execute()?;
		assert_eq!(&*killed.borrow(), &vec!["111".to_string()]);
		cli.verify()
	}

	#[test]
	fn clean_nodes_pid_errors_on_invalid_pids() -> Result<()> {
		let processes = vec![("ink-node".to_string(), "111".to_string(), "30333".to_string())];

		let mut cli = MockCli::new()
			.expect_intro("Remove running nodes")
			.expect_outro_cancel("🚫  Invalid PID(s): 222, 333. No processes killed.");

		let cmd = CleanNodesCommand {
			cli: &mut cli,
			all: false,
			pid: Some(vec!["111".to_string(), "222".to_string(), "333".to_string()]),
			list_nodes: Some(Box::new(move || Ok(processes.clone()))),
			kill_fn: Some(Box::new(|_| unreachable!("kill should not be called"))),
		};

		cmd.execute()?;
		cli.verify()
	}
}
