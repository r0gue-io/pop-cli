// SPDX-License-Identifier: GPL-3.0
#[cfg(test)]
use crate::mock::build_parachain;
use crate::style::{style, Theme};
use clap::Args;
use cliclack::{clear_screen, intro, log::warning, outro, set_theme};
#[cfg(not(test))]
use pop_parachains::build_parachain;
use std::path::PathBuf;

#[derive(Args)]
pub struct BuildParachainCommand {
	#[arg(
		short = 'p',
		long = "path",
		help = "Directory path for your project, [default: current directory]"
	)]
	pub(crate) path: Option<PathBuf>,
}

impl BuildParachainCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building your parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		warning("NOTE: this may take some time...")?;
		build_parachain(&self.path)?;

		outro("Build Completed Successfully!")?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::new::parachain::NewParachainCommand;
	use anyhow::{Error, Result};
	use pop_parachains::{Provider, Template};

	async fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let command = NewParachainCommand {
			name: Some(temp_dir.path().join("test_parachain").to_str().unwrap().to_string()),
			provider: Some(Provider::Pop),
			template: Some(Template::Standard),
			release_tag: None,
			symbol: Some("UNIT".to_string()),
			decimals: Some(12),
			initial_endowment: Some("1u64 << 60".to_string()),
		};
		command.execute().await?;

		Ok(temp_dir)
	}

	#[tokio::test]
	async fn test_build_success() -> Result<()> {
		let temp_dir = setup_test_environment().await?;
		let command = BuildParachainCommand {
			path: Some(PathBuf::from(temp_dir.path().join("test_parachain"))),
		};

		command.execute()?;
		Ok(())
	}
}
