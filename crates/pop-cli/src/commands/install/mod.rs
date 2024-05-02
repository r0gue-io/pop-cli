// SPDX-License-Identifier: GPL-3.0

use anyhow::Context;
use clap::Args;
use tokio::{fs, process::Command};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
/// Setup user environment for substrate development
/// Runs script `scripts/get_substrate.sh`
pub(crate) struct InstallArgs;

impl InstallArgs {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		let temp = tempfile::tempdir()?;
		let scripts_path = temp.path().join("get_substrate.sh");
		let client = reqwest::Client::new();
		let script = client
			.get("https://raw.githubusercontent.com/r0gue-io/pop-cli/pop-install/scripts/get_substrate.sh")
			.send()
			.await
			.context("Network Error: Failed to fetch script from Github")?
			.text()
			.await?;
		fs::write(scripts_path.as_path(), script).await?;
		if cfg!(target_os = "windows") {
			return Ok(cliclack::log::error("Windows is supported only with WSL2")?);
		}
		Command::new("bash").arg(scripts_path).spawn()?;
		Ok(())
	}
}
