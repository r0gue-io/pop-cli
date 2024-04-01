use crate::{db::PopDb, style::{style, Theme}};
use clap::Args;
use cliclack::{clear_screen, intro, outro, set_theme};
use std::path::PathBuf;

use crate::engines::parachain_engine::build_parachain;

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
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building a parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);
		let cached_path = PopDb::open_or_init().get_parachain_path();
		build_parachain(self.path.or(cached_path))?;

		outro("Build Completed Successfully!")?;
		Ok(())
	}
}
