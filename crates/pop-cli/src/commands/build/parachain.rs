use crate::style::{style, Theme};
use clap::Args;
use cliclack::{clear_screen, intro, outro, set_theme};
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
		intro(format!("{}: Building a parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);
		build_parachain(&self.path)?;

		outro("Build Completed Successfully!")?;
		Ok(())
	}
}
