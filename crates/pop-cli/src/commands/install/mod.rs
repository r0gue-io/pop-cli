// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fs;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
/// Setup user environment for substrate development
/// Runs script `scripts/get_substrate.sh`
pub(crate) struct InstallArgs {
	#[command(subcommand)]
	pub command: Option<InstallCommands>,
}

#[derive(Subcommand)]
pub(crate) enum InstallCommands {
	/// Install necessary tools for parachain development.
	/// Same as `pop install` 
	#[clap(alias = "p")]
	Parachain,
	/// Install tools for ink! contract development
	#[clap(alias = "c")]
	Contract,
	/// Install all tools
	#[clap(alias = "a")]
	All,
}

impl InstallArgs {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		let scripts_temp = tempfile::tempdir()?;
		let client = reqwest::Client::new();
		let pre_substrate = client
			.get("https://raw.githubusercontent.com/r0gue-io/pop-cli/main/scripts/get_substrate.sh")
			.send()
			.await?
			.text()
			.await?;
		fs::write(scripts_temp.path().join("substrate.sh"), pre_substrate)?;

		// match self.command {
		// 	InstallCommands::Parachain => install_parachain().await,
		// 	InstallCommands::Contract => install_contract().await,
		// 	InstallCommands::Zombienet => install_zombienet().await,
		// 	InstallCommands::All => install_all().await,
		// }
		Ok(())
	}
}

// async fn install_parachain() -> anyhow::Result<()> {
// 	Ok(())
// }
