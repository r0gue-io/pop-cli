// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self},
	common::{rpc::prompt_to_select_chain_rpc, urls},
};
use anyhow::Result;
use clap::{ArgGroup, Args};
use console::style;
use pop_chains::SupportedChains;
use pop_fork::{
	BlockForkPoint, Blockchain, ExecutorConfig, SignatureMockMode, TxPool,
	rpc_server::{ForkRpcServer, RpcServerConfig},
};
use serde::Serialize;
use std::{
	path::PathBuf,
	process::{Child, Command as StdCommand, Stdio},
	sync::Arc,
	thread,
	time::{Duration, Instant},
};
use tempfile::NamedTempFile;
use url::Url;

/// Timeout for waiting for the detached fork server to become ready.
const DETACH_READY_TIMEOUT_SECS: u64 = 120;
/// Poll interval when checking for fork server readiness.
const DETACH_READY_POLL_MS: u64 = 200;

/// UI messages used across interactive and headless paths.
mod messages {
	/// Intro message for the fork CLI session.
	pub const INTRO: &str = "Forking chain";
	/// Intro message for detached fork mode.
	pub const INTRO_DETACHED: &str = "Forking chain (detached mode)";
	/// Prompt to stop the server.
	pub const PRESS_CTRL_C: &str = "Press Ctrl+C to stop.";
	/// Shutdown message.
	pub const SHUTTING_DOWN: &str = "Shutting down...";

	/// Format "Forking `endpoint`..." progress message.
	pub fn forking(endpoint: &impl std::fmt::Display) -> String {
		format!("Forking {endpoint}...")
	}

	/// Format "Dev accounts funded on `chain`" message.
	pub fn dev_accounts_funded(chain_name: &str) -> String {
		format!("Dev accounts funded on {chain_name}")
	}

	/// Format "Forked `chain` at block #N -> `ws_url`" message.
	pub fn forked(chain_name: &str, block_number: u32, ws_url: &str) -> String {
		format!("Forked {chain_name} at block #{block_number} -> {ws_url}")
	}
}

/// Arguments for the fork command.
#[derive(Args, Clone, Default, Serialize)]
#[command(group = ArgGroup::new("source").args(["chain", "endpoint"]))]
pub(crate) struct ForkArgs {
	/// Well-known chain to fork (e.g., paseo, polkadot, asset-hub, asset-hub-polkadot).
	#[arg(value_enum, index = 1)]
	#[serde(skip)]
	pub chain: Option<SupportedChains>,

	/// RPC endpoint to fork from.
	#[arg(short = 'e', long = "endpoint")]
	pub endpoint: Option<String>,

	/// Path to persist SQLite cache. If not specified, uses in-memory cache.
	#[arg(short, long)]
	pub cache: Option<PathBuf>,

	/// Port for the RPC server. Auto-finds from 9944 if not specified.
	#[arg(short, long)]
	pub port: Option<u16>,

	/// Accept all signatures as valid (default: only magic signatures 0xdeadbeef).
	/// Use this for maximum flexibility when testing.
	#[arg(long = "mock-all-signatures")]
	pub mock_all_signatures: bool,

	/// Fund well-known dev accounts (Alice, Bob, Charlie, Dave, Eve, Ferdie)
	/// and set Alice as sudo (if the chain has the Sudo pallet).
	#[arg(long)]
	pub dev: bool,

	/// Run the fork in the background and return immediately.
	#[arg(short, long)]
	pub detach: bool,

	/// Fork at a specific block number. If not specified, forks at the latest finalized block.
	#[arg(long)]
	pub at: Option<u32>,

	/// Internal flag: run as background server (used by detach mode).
	#[arg(long, hide = true, requires = "endpoint")]
	#[serde(skip)]
	pub serve: bool,

	/// Internal flag: path to write readiness info to (used by detach mode).
	#[arg(long, hide = true, requires = "serve")]
	#[serde(skip)]
	pub ready_file: Option<PathBuf>,
}

pub(crate) struct Command;

impl Command {
	pub(crate) async fn execute(
		args: &mut ForkArgs,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		// --serve is an internal flag used by spawn_detached; it always receives the
		// endpoint via CLI args, so no prompting or intro is needed.
		if args.serve {
			if args.endpoint.is_none() {
				anyhow::bail!("--serve requires --endpoint");
			}
			return Self::run_server(args).await;
		}

		// Show intro first so the cliclack session is set up before any prompts.
		if args.detach {
			cli.intro(messages::INTRO_DETACHED)?;
		} else {
			cli.intro(messages::INTRO)?;
		}

		// When a well-known chain is specified, try each RPC URL with fallback.
		if let Some(chain) = args.chain {
			return Self::execute_with_fallback(args, &chain, cli).await;
		}

		// Prompt for endpoint if none provided.
		if args.endpoint.is_none() {
			let url = prompt_to_select_chain_rpc(
				"Which chain would you like to fork? (type to filter)",
				"Type the chain RPC URL",
				urls::LOCAL,
				|_| true,
				cli,
			)
			.await?;
			args.endpoint = Some(url.to_string());
		}

		if args.detach {
			return Self::spawn_detached(args, cli);
		}

		Self::run_interactive(args, cli).await
	}

	/// Try each RPC URL for a well-known chain, falling back on failure.
	async fn execute_with_fallback(
		args: &ForkArgs,
		chain: &SupportedChains,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		let rpc_urls = chain.rpc_urls();
		let mut last_error = None;

		for rpc_url in rpc_urls {
			let resolved =
				ForkArgs { endpoint: Some(rpc_url.to_string()), chain: None, ..args.clone() };

			match Self::execute_resolved(&resolved, cli).await {
				Ok(()) => return Ok(()),
				Err(e) => {
					cli.warning(format!("{} did not respond, trying next endpoint...", rpc_url))?;
					last_error = Some(e);
				},
			}
		}

		Err(last_error
			.unwrap_or_else(|| anyhow::anyhow!("No RPC endpoints available for {}", chain)))
	}

	/// Execute with an already-resolved endpoint (no chain fallback).
	async fn execute_resolved(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		if args.detach {
			return Self::spawn_detached(args, cli);
		}
		Self::run_interactive(args, cli).await
	}

	/// Spawn a detached background process and return immediately.
	fn spawn_detached(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		// Create log file that persists after we exit.
		let log_file = NamedTempFile::new()?;
		let log_path = log_file.path().to_path_buf();
		let ready_path = log_path.with_extension("ready");

		// Build command args with --ready-file for readiness signaling.
		let mut cmd_args = Self::build_serve_args(args);
		cmd_args.push("--ready-file".to_string());
		cmd_args.push(ready_path.to_string_lossy().to_string());

		// Spawn subprocess with output redirected to log file.
		let mut child = Self::spawn_server_process(&cmd_args, &log_path)?;
		let pid = child.id();

		// Keep log file persistent (don't delete on drop).
		log_file.keep()?;

		// Wait for the server to signal readiness and display fork info.
		match Self::wait_for_ready(&ready_path, &mut child, cli) {
			Ok(()) => {
				let _ = std::fs::remove_file(&ready_path);
				cli.success(format!("Fork started with PID {}", pid))?;
				cli.info(format!("Log file: {}", log_path.display()))?;
				cli.outro(format!(
					"Run `kill -9 {}` or `pop clean node -p {}` to stop.",
					pid, pid
				))?;
				Ok(())
			},
			Err(e) => {
				let _ = child.kill();
				let _ = child.wait();
				let _ = std::fs::remove_file(&ready_path);
				Err(e)
			},
		}
	}

	/// Wait for the detached server process to signal readiness.
	/// Polls a readiness file written by the child process.
	fn wait_for_ready(
		ready_path: &std::path::Path,
		child: &mut Child,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		let timeout = Duration::from_secs(DETACH_READY_TIMEOUT_SECS);
		let poll_interval = Duration::from_millis(DETACH_READY_POLL_MS);
		let start = Instant::now();

		loop {
			// Check if child process has exited (likely an error).
			if let Some(status) = child.try_wait()? {
				anyhow::bail!(
					"Fork process exited unexpectedly (status: {}). Check the log file for details.",
					status
				);
			}

			// Check for readiness file.
			if let Ok(content) = std::fs::read_to_string(ready_path) &&
				!content.is_empty()
			{
				for line in content.lines() {
					cli.success(line)?;
				}
				return Ok(());
			}

			if start.elapsed() > timeout {
				anyhow::bail!(
					"Timed out waiting for fork to be ready. Check the log file for details."
				);
			}

			thread::sleep(poll_interval);
		}
	}

	/// Spawn the server process. Extracted for testability.
	fn spawn_server_process(cmd_args: &[String], log_path: &PathBuf) -> Result<Child> {
		let exe = std::env::current_exe()?;
		let log_file_handle = std::fs::File::create(log_path)?;
		let child = StdCommand::new(exe)
			.args(cmd_args)
			.stdout(log_file_handle.try_clone()?)
			.stderr(log_file_handle)
			.stdin(Stdio::null())
			.spawn()?;
		Ok(child)
	}

	/// Run as a background server (called via --serve flag).
	/// Output goes to log file, waits for termination signal.
	async fn run_server(args: &ForkArgs) -> Result<()> {
		let endpoint: Url =
			args.endpoint.as_ref().expect("endpoint required for --serve").parse()?;

		let executor_config = ExecutorConfig {
			signature_mock: if args.mock_all_signatures {
				SignatureMockMode::AlwaysValid
			} else {
				SignatureMockMode::MagicSignature
			},
			..Default::default()
		};

		let fork_point = args.at.map(BlockForkPoint::from);

		log::info!("{}", messages::forking(&endpoint));

		let blockchain = Blockchain::fork_with_config(
			&endpoint,
			args.cache.as_deref(),
			fork_point,
			executor_config,
		)
		.await?;

		if args.dev {
			blockchain.initialize_dev_accounts().await?;
			log::info!("{}", messages::dev_accounts_funded(blockchain.chain_name()));
		}

		let txpool = Arc::new(TxPool::new());
		let server_config = RpcServerConfig { port: args.port, ..Default::default() };
		let server = ForkRpcServer::start(blockchain.clone(), txpool, server_config).await?;

		let forked_msg = messages::forked(
			blockchain.chain_name(),
			blockchain.fork_point_number(),
			&server.ws_url(),
		);
		log::info!("{forked_msg}");

		// Signal readiness to the parent process (detach mode).
		if let Some(ready_path) = &args.ready_file {
			std::fs::write(ready_path, &forked_msg)?;
		}

		log::info!("Server running. Waiting for termination signal...");

		// Wait for termination signal
		tokio::signal::ctrl_c().await?;

		log::info!("{}", messages::SHUTTING_DOWN);
		server.stop().await;
		let _ = blockchain.clear_local_storage().await;

		log::info!("Shutdown complete.");
		Ok(())
	}

	/// Run interactively with CLI output (default mode).
	async fn run_interactive(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		let endpoint: Url = args.endpoint.as_ref().expect("endpoint required").parse()?;

		let executor_config = ExecutorConfig {
			signature_mock: if args.mock_all_signatures {
				SignatureMockMode::AlwaysValid
			} else {
				SignatureMockMode::MagicSignature
			},
			..Default::default()
		};

		let fork_point = args.at.map(BlockForkPoint::from);

		cli.info(messages::forking(&endpoint))?;

		let blockchain = Blockchain::fork_with_config(
			&endpoint,
			args.cache.as_deref(),
			fork_point,
			executor_config,
		)
		.await?;

		if args.dev {
			blockchain.initialize_dev_accounts().await?;
			cli.info(messages::dev_accounts_funded(blockchain.chain_name()))?;
		}

		let txpool = Arc::new(TxPool::new());
		let server_config = RpcServerConfig { port: args.port, ..Default::default() };
		let server = ForkRpcServer::start(blockchain.clone(), txpool, server_config).await?;

		let ws = server.ws_url();
		cli.success(format!(
			"{}\n{}\n{}",
			messages::forked(blockchain.chain_name(), blockchain.fork_point_number(), &ws),
			style(format!("  polkadot.js: https://polkadot.js.org/apps/?rpc={ws}#/explorer")).dim(),
			style(format!(
				"  papi:        https://dev.papi.how/explorer#networkId=custom&endpoint={ws}"
			))
			.dim(),
		))?;

		cli.info(messages::PRESS_CTRL_C)?;

		tokio::signal::ctrl_c().await?;

		cli.info(messages::SHUTTING_DOWN)?;
		server.stop().await;
		if let Err(e) = blockchain.clear_local_storage().await {
			cli.warning(format!("Failed to clear local storage: {}", e))?;
		}

		cli.outro("Done.")?;
		Ok(())
	}

	/// Build command arguments for spawning a serve subprocess.
	/// Extracted for testability.
	fn build_serve_args(args: &ForkArgs) -> Vec<String> {
		let mut cmd_args = vec!["fork".to_string()];
		if let Some(endpoint) = &args.endpoint {
			cmd_args.push("-e".to_string());
			cmd_args.push(endpoint.clone());
		}
		if let Some(cache) = &args.cache {
			cmd_args.push("--cache".to_string());
			cmd_args.push(cache.to_string_lossy().to_string());
		}
		if let Some(port) = args.port {
			cmd_args.push("--port".to_string());
			cmd_args.push(port.to_string());
		}
		if args.mock_all_signatures {
			cmd_args.push("--mock-all-signatures".to_string());
		}
		if args.dev {
			cmd_args.push("--dev".to_string());
		}
		if let Some(at) = args.at {
			cmd_args.push("--at".to_string());
			cmd_args.push(at.to_string());
		}
		cmd_args.push("--serve".to_string());
		cmd_args
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;

	#[tokio::test(flavor = "multi_thread")]
	async fn execute_prompts_when_no_endpoint_selects_local() {
		let mut args = ForkArgs::default();
		let mut cli = MockCli::new().expect_select(
			"Which chain would you like to fork? (type to filter)",
			None,
			true,
			Some(vec![
				("Local".to_string(), "Local node (ws://localhost:9944)".to_string()),
				("Custom".to_string(), "Type the chain URL manually".to_string()),
			]),
			0, // select Local
			None,
		);
		// execute will fail connecting, but the prompt should populate endpoint
		let _ = Command::execute(&mut args, &mut cli).await;
		assert_eq!(args.endpoint, Some("ws://localhost:9944/".to_string()));
		cli.verify().unwrap();
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn execute_prompts_when_no_endpoint_selects_custom() {
		let mut args = ForkArgs::default();
		let mut cli = MockCli::new()
			.expect_select(
				"Which chain would you like to fork? (type to filter)",
				None,
				true,
				Some(vec![
					("Local".to_string(), "Local node (ws://localhost:9944)".to_string()),
					("Custom".to_string(), "Type the chain URL manually".to_string()),
				]),
				1, // select Custom
				None,
			)
			.expect_input("Type the chain RPC URL", "ws://127.0.0.1:1".to_string());
		let _ = Command::execute(&mut args, &mut cli).await;
		assert_eq!(args.endpoint, Some("ws://127.0.0.1:1/".to_string()));
		cli.verify().unwrap();
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn execute_skips_prompt_when_endpoint_provided() {
		let mut args =
			ForkArgs { endpoint: Some("ws://127.0.0.1:1".to_string()), ..Default::default() };
		// No select expectation -- prompt should not be triggered
		let mut cli = MockCli::new();
		let _ = Command::execute(&mut args, &mut cli).await;
		assert_eq!(args.endpoint, Some("ws://127.0.0.1:1".to_string()));
		cli.verify().unwrap();
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn execute_errors_when_serve_without_endpoint() {
		let mut args = ForkArgs { serve: true, ..Default::default() };
		let mut cli = MockCli::new();
		let err = Command::execute(&mut args, &mut cli).await.unwrap_err();
		assert!(err.to_string().contains("--serve requires --endpoint"));
		cli.verify().unwrap();
	}

	#[test]
	fn build_serve_args_minimal() {
		let args =
			ForkArgs { endpoint: Some("wss://rpc.polkadot.io".to_string()), ..Default::default() };
		let result = Command::build_serve_args(&args);
		assert_eq!(result, vec!["fork", "-e", "wss://rpc.polkadot.io", "--serve"]);
	}

	#[test]
	fn build_serve_args_full() {
		let args = ForkArgs {
			endpoint: Some("wss://rpc.polkadot.io".to_string()),
			cache: Some(PathBuf::from("/tmp/cache.db")),
			port: Some(9000),
			mock_all_signatures: true,
			dev: true,
			at: Some(100),
			detach: true,
			serve: false,
			chain: None,
			ready_file: None,
		};
		let result = Command::build_serve_args(&args);
		assert_eq!(
			result,
			vec![
				"fork",
				"-e",
				"wss://rpc.polkadot.io",
				"--cache",
				"/tmp/cache.db",
				"--port",
				"9000",
				"--mock-all-signatures",
				"--dev",
				"--at",
				"100",
				"--serve"
			]
		);
	}

	#[test]
	fn build_serve_args_with_at() {
		let args = ForkArgs {
			endpoint: Some("wss://rpc.polkadot.io".to_string()),
			at: Some(5000),
			..Default::default()
		};
		let result = Command::build_serve_args(&args);
		assert_eq!(result, vec!["fork", "-e", "wss://rpc.polkadot.io", "--at", "5000", "--serve"]);
	}

	#[test]
	fn build_serve_args_without_at() {
		let args =
			ForkArgs { endpoint: Some("wss://rpc.polkadot.io".to_string()), ..Default::default() };
		let result = Command::build_serve_args(&args);
		assert!(!result.contains(&"--at".to_string()));
	}

	#[test]
	fn build_serve_args_includes_serve_not_detach() {
		let args = ForkArgs {
			endpoint: Some("wss://test.io".to_string()),
			detach: true,
			..Default::default()
		};
		let result = Command::build_serve_args(&args);
		assert!(result.contains(&"--serve".to_string()));
		assert!(!result.contains(&"--detach".to_string()));
	}

	#[test]
	fn build_serve_args_excludes_ready_file() {
		let args = ForkArgs {
			endpoint: Some("wss://test.io".to_string()),
			ready_file: Some(PathBuf::from("/tmp/test.ready")),
			..Default::default()
		};
		let result = Command::build_serve_args(&args);
		assert!(!result.contains(&"--ready-file".to_string()));
	}

	#[test]
	fn wait_for_ready_succeeds_with_ready_file() {
		let dir = tempfile::tempdir().unwrap();
		let ready_path = dir.path().join("test.ready");
		std::fs::write(&ready_path, "Forked paseo at block #100 -> ws://127.0.0.1:9945").unwrap();

		let mut child = StdCommand::new("sleep")
			.arg("60")
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();

		let mut cli =
			MockCli::new().expect_success("Forked paseo at block #100 -> ws://127.0.0.1:9945");

		let result = Command::wait_for_ready(&ready_path, &mut child, &mut cli);
		assert!(result.is_ok());
		cli.verify().unwrap();

		let _ = child.kill();
		let _ = child.wait();
	}

	#[test]
	fn wait_for_ready_fails_when_child_exits_with_error() {
		let dir = tempfile::tempdir().unwrap();
		let ready_path = dir.path().join("test.ready");

		let mut child = StdCommand::new("false")
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();

		// Give the process time to exit.
		thread::sleep(Duration::from_millis(100));

		let mut cli = MockCli::new();

		let result = Command::wait_for_ready(&ready_path, &mut child, &mut cli);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("Fork process exited unexpectedly"));
	}

	#[test]
	fn wait_for_ready_fails_when_child_exits_cleanly_without_ready_file() {
		let dir = tempfile::tempdir().unwrap();
		let ready_path = dir.path().join("test.ready");

		let mut child = StdCommand::new("true")
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();

		// Give the process time to exit.
		thread::sleep(Duration::from_millis(100));

		let mut cli = MockCli::new();

		let result = Command::wait_for_ready(&ready_path, &mut child, &mut cli);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("Fork process exited unexpectedly"));
	}
}
