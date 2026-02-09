// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self};
use anyhow::Result;
use clap::{ArgGroup, Args, ValueEnum};
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

/// Log level for fork command output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize)]
pub enum LogLevel {
	/// Disable all logging.
	Off,
	/// Error level only.
	Error,
	/// Warnings and errors.
	Warn,
	/// Informational messages (default).
	#[default]
	Info,
	/// Debug messages.
	Debug,
	/// Trace messages.
	Trace,
}

impl LogLevel {
	/// Convert to log::LevelFilter.
	pub fn to_level_filter(self) -> log::LevelFilter {
		match self {
			LogLevel::Off => log::LevelFilter::Off,
			LogLevel::Error => log::LevelFilter::Error,
			LogLevel::Warn => log::LevelFilter::Warn,
			LogLevel::Info => log::LevelFilter::Info,
			LogLevel::Debug => log::LevelFilter::Debug,
			LogLevel::Trace => log::LevelFilter::Trace,
		}
	}
}

/// Arguments for the fork command.
#[derive(Args, Clone, Default, Serialize)]
#[command(group = ArgGroup::new("source").args(["chain", "endpoints"]).required(true))]
pub(crate) struct ForkArgs {
	/// Well-known chain to fork (e.g., paseo, polkadot, kusama, westend).
	#[arg(value_enum, index = 1)]
	#[serde(skip)]
	pub chain: Option<SupportedChains>,

	/// RPC endpoint(s) to fork from. Use multiple times for multiple chains.
	#[arg(short = 'e', long = "endpoint")]
	pub endpoints: Vec<String>,

	/// Path to persist SQLite cache. If not specified, uses in-memory cache.
	#[arg(short, long)]
	pub cache: Option<PathBuf>,

	/// Starting port for RPC server(s). Auto-finds from 8000 if not specified.
	#[arg(short, long)]
	pub port: Option<u16>,

	/// Accept all signatures as valid (default: only magic signatures 0xdeadbeef).
	/// Use this for maximum flexibility when testing.
	#[arg(long = "mock-all-signatures")]
	pub mock_all_signatures: bool,

	/// Log level for internal block building operations.
	#[arg(long = "log-level", value_enum, default_value = "info")]
	pub log_level: LogLevel,

	/// Run the fork in the background and return immediately.
	#[arg(short, long)]
	pub detach: bool,

	/// Fork at a specific block number. If not specified, forks at the latest finalized block.
	#[arg(long)]
	pub at: Option<u32>,

	/// Internal flag: run as background server (used by detach mode).
	#[arg(long, hide = true)]
	#[serde(skip)]
	pub serve: bool,

	/// Internal flag: path to write readiness info to (used by detach mode).
	#[arg(long, hide = true, requires = "serve")]
	#[serde(skip)]
	pub ready_file: Option<PathBuf>,
}

pub(crate) struct Command;

impl Command {
	pub(crate) async fn execute(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		// Serve mode is always called with resolved endpoints (no chain).
		if args.serve {
			return Self::run_server(args).await;
		}

		// When a well-known chain is specified, try each RPC URL with fallback.
		if let Some(chain) = &args.chain {
			return Self::execute_with_fallback(args, chain, cli).await;
		}

		// Direct endpoint mode (existing behavior).
		Self::execute_resolved(args, cli).await
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
				ForkArgs { endpoints: vec![rpc_url.to_string()], chain: None, ..args.clone() };

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

	/// Execute with already-resolved endpoints (no chain fallback).
	async fn execute_resolved(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		if args.detach {
			return Self::spawn_detached(args, cli);
		}
		Self::run_interactive(args, cli).await
	}

	/// Spawn a detached background process and return immediately.
	fn spawn_detached(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		cli.intro("Forking chain(s) (detached mode)")?;

		for endpoint in &args.endpoints {
			cli.info(format!("Forking {}...", endpoint))?;
		}

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
				if !status.success() {
					anyhow::bail!(
						"Fork process exited with status {}. Check the log file for details.",
						status
					);
				}
				break;
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

		Ok(())
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
		let endpoints: Vec<Url> = args
			.endpoints
			.iter()
			.map(|e| e.parse::<Url>())
			.collect::<std::result::Result<Vec<_>, _>>()?;

		let executor_config = ExecutorConfig {
			signature_mock: if args.mock_all_signatures {
				SignatureMockMode::AlwaysValid
			} else {
				SignatureMockMode::MagicSignature
			},
			max_log_level: args.log_level as u32,
			..Default::default()
		};

		let fork_point = args.at.map(BlockForkPoint::from);

		let mut servers: Vec<(String, Arc<Blockchain>, ForkRpcServer)> = Vec::new();
		let mut current_port = args.port;

		for (i, endpoint) in endpoints.iter().enumerate() {
			println!("Forking {}...", endpoint);

			let cache_path = Self::resolve_cache_path(&args.cache, endpoints.len(), i);

			let blockchain = Blockchain::fork_with_config(
				endpoint,
				cache_path.as_deref(),
				fork_point,
				executor_config.clone(),
			)
			.await?;

			let txpool = Arc::new(TxPool::new());
			let server_config = RpcServerConfig { port: current_port, max_connections: 100 };
			let server = ForkRpcServer::start(blockchain.clone(), txpool, server_config).await?;

			if current_port.is_some() {
				current_port = Some(server.addr().port() + 1);
			}

			println!(
				"Forked {} at block #{} -> {}",
				blockchain.chain_name(),
				blockchain.fork_point_number(),
				server.ws_url()
			);

			servers.push((blockchain.chain_name().to_string(), blockchain, server));
		}

		// Signal readiness to the parent process (detach mode).
		if let Some(ready_path) = &args.ready_file {
			let info: Vec<String> = servers
				.iter()
				.map(|(_, blockchain, server)| {
					format!(
						"Forked {} at block #{} -> {}",
						blockchain.chain_name(),
						blockchain.fork_point_number(),
						server.ws_url()
					)
				})
				.collect();
			std::fs::write(ready_path, info.join("\n"))?;
		}

		println!("Server running. Waiting for termination signal...");

		// Wait for termination signal
		tokio::signal::ctrl_c().await?;

		println!("Shutting down...");
		for (_, blockchain, server) in servers {
			server.stop().await;
			let _ = blockchain.clear_local_storage().await;
		}

		println!("Shutdown complete.");
		Ok(())
	}

	/// Run interactively with CLI output (default mode).
	async fn run_interactive(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		cli.intro("Forking chain(s)")?;

		let endpoints: Vec<Url> = args
			.endpoints
			.iter()
			.map(|e| e.parse::<Url>())
			.collect::<std::result::Result<Vec<_>, _>>()?;

		let executor_config = ExecutorConfig {
			signature_mock: if args.mock_all_signatures {
				SignatureMockMode::AlwaysValid
			} else {
				SignatureMockMode::MagicSignature
			},
			max_log_level: args.log_level as u32,
			..Default::default()
		};

		let fork_point = args.at.map(BlockForkPoint::from);

		let mut servers: Vec<(String, Arc<Blockchain>, ForkRpcServer)> = Vec::new();
		let mut current_port = args.port;

		for (i, endpoint) in endpoints.iter().enumerate() {
			cli.info(format!("Forking {}...", endpoint))?;

			let cache_path = Self::resolve_cache_path(&args.cache, endpoints.len(), i);

			let blockchain = Blockchain::fork_with_config(
				endpoint,
				cache_path.as_deref(),
				fork_point,
				executor_config.clone(),
			)
			.await?;

			let txpool = Arc::new(TxPool::new());

			let server_config = RpcServerConfig { port: current_port, max_connections: 100 };

			let server = ForkRpcServer::start(blockchain.clone(), txpool, server_config).await?;

			if current_port.is_some() {
				current_port = Some(server.addr().port() + 1);
			}

			cli.success(format!(
				"Forked {} at block #{} -> {}",
				blockchain.chain_name(),
				blockchain.fork_point_number(),
				server.ws_url()
			))?;

			servers.push((blockchain.chain_name().to_string(), blockchain, server));
		}

		cli.info("Press Ctrl+C to stop.")?;

		tokio::signal::ctrl_c().await?;

		cli.info("Shutting down...")?;
		for (_, blockchain, server) in servers {
			server.stop().await;
			// Clear local storage to remove temporary state from cache
			if let Err(e) = blockchain.clear_local_storage().await {
				cli.warning(format!("Failed to clear local storage: {}", e))?;
			}
		}

		cli.outro("Done.")?;
		Ok(())
	}

	/// Resolve cache path for a specific chain index (handles multiple chains).
	fn resolve_cache_path(
		cache: &Option<PathBuf>,
		num_endpoints: usize,
		index: usize,
	) -> Option<PathBuf> {
		cache.as_ref().map(|p| {
			if num_endpoints > 1 {
				let stem = p.file_stem().unwrap_or_default().to_string_lossy();
				let ext = p.extension().map(|e| e.to_string_lossy()).unwrap_or_default();
				if ext.is_empty() {
					p.with_file_name(format!("{}_{}", stem, index))
				} else {
					p.with_file_name(format!("{}_{}.{}", stem, index, ext))
				}
			} else {
				p.clone()
			}
		})
	}

	/// Build command arguments for spawning a serve subprocess.
	/// Extracted for testability.
	fn build_serve_args(args: &ForkArgs) -> Vec<String> {
		let mut cmd_args = vec!["fork".to_string()];
		for endpoint in &args.endpoints {
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
		if let Some(at) = args.at {
			cmd_args.push("--at".to_string());
			cmd_args.push(at.to_string());
		}
		cmd_args.push("--log-level".to_string());
		cmd_args.push(format!("{:?}", args.log_level).to_lowercase());
		cmd_args.push("--serve".to_string());
		cmd_args
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;

	#[test]
	fn resolve_cache_path_single_endpoint_no_extension() {
		let cache = Some(PathBuf::from("/tmp/cache"));
		let result = Command::resolve_cache_path(&cache, 1, 0);
		assert_eq!(result, Some(PathBuf::from("/tmp/cache")));
	}

	#[test]
	fn resolve_cache_path_single_endpoint_with_extension() {
		let cache = Some(PathBuf::from("/tmp/cache.db"));
		let result = Command::resolve_cache_path(&cache, 1, 0);
		assert_eq!(result, Some(PathBuf::from("/tmp/cache.db")));
	}

	#[test]
	fn resolve_cache_path_multiple_endpoints_no_extension() {
		let cache = Some(PathBuf::from("/tmp/cache"));
		assert_eq!(Command::resolve_cache_path(&cache, 2, 0), Some(PathBuf::from("/tmp/cache_0")));
		assert_eq!(Command::resolve_cache_path(&cache, 2, 1), Some(PathBuf::from("/tmp/cache_1")));
	}

	#[test]
	fn resolve_cache_path_multiple_endpoints_with_extension() {
		let cache = Some(PathBuf::from("/tmp/cache.db"));
		assert_eq!(
			Command::resolve_cache_path(&cache, 3, 0),
			Some(PathBuf::from("/tmp/cache_0.db"))
		);
		assert_eq!(
			Command::resolve_cache_path(&cache, 3, 1),
			Some(PathBuf::from("/tmp/cache_1.db"))
		);
		assert_eq!(
			Command::resolve_cache_path(&cache, 3, 2),
			Some(PathBuf::from("/tmp/cache_2.db"))
		);
	}

	#[test]
	fn resolve_cache_path_none() {
		let result = Command::resolve_cache_path(&None, 2, 0);
		assert_eq!(result, None);
	}

	#[test]
	fn build_serve_args_minimal() {
		let args =
			ForkArgs { endpoints: vec!["wss://rpc.polkadot.io".to_string()], ..Default::default() };
		let result = Command::build_serve_args(&args);
		assert_eq!(
			result,
			vec!["fork", "-e", "wss://rpc.polkadot.io", "--log-level", "info", "--serve"]
		);
	}

	#[test]
	fn build_serve_args_full() {
		let args = ForkArgs {
			endpoints: vec![
				"wss://rpc.polkadot.io".to_string(),
				"wss://kusama-rpc.polkadot.io".to_string(),
			],
			cache: Some(PathBuf::from("/tmp/cache.db")),
			port: Some(9000),
			mock_all_signatures: true,
			log_level: LogLevel::Debug,
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
				"-e",
				"wss://kusama-rpc.polkadot.io",
				"--cache",
				"/tmp/cache.db",
				"--port",
				"9000",
				"--mock-all-signatures",
				"--at",
				"100",
				"--log-level",
				"debug",
				"--serve"
			]
		);
	}

	#[test]
	fn build_serve_args_with_at() {
		let args = ForkArgs {
			endpoints: vec!["wss://rpc.polkadot.io".to_string()],
			at: Some(5000),
			..Default::default()
		};
		let result = Command::build_serve_args(&args);
		assert_eq!(
			result,
			vec![
				"fork",
				"-e",
				"wss://rpc.polkadot.io",
				"--at",
				"5000",
				"--log-level",
				"info",
				"--serve"
			]
		);
	}

	#[test]
	fn build_serve_args_without_at() {
		let args =
			ForkArgs { endpoints: vec!["wss://rpc.polkadot.io".to_string()], ..Default::default() };
		let result = Command::build_serve_args(&args);
		assert!(!result.contains(&"--at".to_string()));
	}

	#[test]
	fn build_serve_args_includes_serve_not_detach() {
		let args = ForkArgs {
			endpoints: vec!["wss://test.io".to_string()],
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
			endpoints: vec!["wss://test.io".to_string()],
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
	fn wait_for_ready_displays_multiple_fork_lines() {
		let dir = tempfile::tempdir().unwrap();
		let ready_path = dir.path().join("test.ready");
		std::fs::write(
			&ready_path,
			"Forked polkadot at block #100 -> ws://127.0.0.1:9945\n\
			 Forked kusama at block #200 -> ws://127.0.0.1:9946",
		)
		.unwrap();

		let mut child = StdCommand::new("sleep")
			.arg("60")
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();

		let mut cli = MockCli::new()
			.expect_success("Forked polkadot at block #100 -> ws://127.0.0.1:9945")
			.expect_success("Forked kusama at block #200 -> ws://127.0.0.1:9946");

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
		assert!(result.unwrap_err().to_string().contains("Fork process exited with status"));
	}

	#[test]
	fn wait_for_ready_returns_ok_when_child_exits_cleanly() {
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
		assert!(result.is_ok());
	}
}
