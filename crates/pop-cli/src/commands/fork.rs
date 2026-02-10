// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self};
use anyhow::Result;
use clap::Args;
use pop_fork::{
	Blockchain, ExecutorConfig, SignatureMockMode, TxPool,
	rpc_server::{ForkRpcServer, RpcServerConfig},
};
use serde::Serialize;
use std::{
	path::PathBuf,
	process::{Child, Command as StdCommand, Stdio},
	sync::Arc,
};
use tempfile::NamedTempFile;
use url::Url;

/// Arguments for the fork command.
#[derive(Args, Clone, Default, Serialize)]
pub(crate) struct ForkArgs {
	/// RPC endpoint(s) to fork from. Use multiple times for multiple chains.
	#[arg(short = 'e', long = "endpoint", required = true)]
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

	/// Run the fork in the background and return immediately.
	#[arg(short, long)]
	pub detach: bool,

	/// Internal flag: run as background server (used by detach mode).
	#[arg(long, hide = true)]
	#[serde(skip)]
	pub serve: bool,
}

pub(crate) struct Command;

impl Command {
	pub(crate) async fn execute(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		if args.detach {
			return Self::spawn_detached(args, cli);
		}

		if args.serve {
			return Self::run_server(args).await;
		}

		Self::run_interactive(args, cli).await
	}

	/// Spawn a detached background process and return immediately.
	fn spawn_detached(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
		cli.intro("Forking chain(s) (detached mode)")?;

		// Create log file that persists after we exit
		let log_file = NamedTempFile::new()?;
		let log_path = log_file.path().to_path_buf();

		// Build command args: same as current but with --serve instead of --detach
		let cmd_args = Self::build_serve_args(args);

		// Spawn subprocess with output redirected to log file
		let child = Self::spawn_server_process(&cmd_args, &log_path)?;
		let pid = child.id();

		// Keep log file persistent (don't delete on drop)
		log_file.keep()?;

		cli.success(format!("Fork started with PID {}", pid))?;
		cli.info(format!("Log file: {}", log_path.display()))?;
		cli.outro(format!("Run `kill -9 {}` or `pop clean node -p {}` to stop.", pid, pid))?;

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
			..Default::default()
		};

		let mut servers: Vec<(String, Arc<Blockchain>, ForkRpcServer)> = Vec::new();
		let mut current_port = args.port;

		for (i, endpoint) in endpoints.iter().enumerate() {
			println!("Forking {}...", endpoint);

			let cache_path = Self::resolve_cache_path(&args.cache, endpoints.len(), i);

			let blockchain = Blockchain::fork_with_config(
				endpoint,
				cache_path.as_deref(),
				None,
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
			..Default::default()
		};

		let mut servers: Vec<(String, Arc<Blockchain>, ForkRpcServer)> = Vec::new();
		let mut current_port = args.port;

		for (i, endpoint) in endpoints.iter().enumerate() {
			cli.info(format!("Forking {}...", endpoint))?;

			let cache_path = Self::resolve_cache_path(&args.cache, endpoints.len(), i);

			let blockchain = Blockchain::fork_with_config(
				endpoint,
				cache_path.as_deref(),
				None,
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
		cmd_args.push("--serve".to_string());
		cmd_args
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
		assert_eq!(result, vec!["fork", "-e", "wss://rpc.polkadot.io", "--serve"]);
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
			detach: true, // Should not appear in serve args
			serve: false,
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
				"--serve"
			]
		);
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
}
