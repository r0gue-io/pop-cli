// SPDX-License-Identifier: GPL-3.0
#![allow(missing_docs)]

use anyhow::Result;
use clap::Parser;
use pop_cli::Cli;
#[cfg(feature = "telemetry")]
use pop_cli::init;
#[cfg(feature = "telemetry")]
use pop_telemetry::record_cli_command;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "telemetry")]
	let maybe_tel = init().unwrap_or(None);

	let mut cli = Cli::parse();
	#[cfg(feature = "telemetry")]
	let event = cli.event_name();

	let result = cli.execute().await;

	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel {
		let data = cli.command_payload();
		let _ = record_cli_command(tel, &event, data).await;
	}

	result
}
