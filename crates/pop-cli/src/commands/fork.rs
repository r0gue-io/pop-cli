// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self};
use anyhow::Result;
use clap::{Args, ValueEnum};
use pop_fork::{
	Blockchain, ExecutorConfig, SignatureMockMode, TxPool,
	rpc_server::{ForkRpcServer, RpcServerConfig},
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use url::Url;

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

	/// Log level for internal block building operations.
	#[arg(long = "log-level", value_enum, default_value = "info")]
	pub log_level: LogLevel,
}

pub(crate) struct Command;

impl Command {
	pub(crate) async fn execute(args: &ForkArgs, cli: &mut impl cli::traits::Cli) -> Result<()> {
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

		let mut servers: Vec<(String, Arc<Blockchain>, ForkRpcServer)> = Vec::new();
		let mut current_port = args.port;

		for (i, endpoint) in endpoints.iter().enumerate() {
			cli.info(format!("Forking {}...", endpoint))?;

			// Handle cache path for multiple chains
			let cache_path = args.cache.as_ref().map(|p| {
				if endpoints.len() > 1 {
					let stem = p.file_stem().unwrap_or_default().to_string_lossy();
					let ext = p.extension().map(|e| e.to_string_lossy()).unwrap_or_default();
					if ext.is_empty() {
						p.with_file_name(format!("{}_{}", stem, i))
					} else {
						p.with_file_name(format!("{}_{}.{}", stem, i, ext))
					}
				} else {
					p.clone()
				}
			});

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
}
