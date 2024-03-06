use clap::Args;
use std::path::PathBuf;
use cliclack::{clear_screen,intro, set_theme, outro};
use crate::style::{style, Theme};

use crate::engines::parachain_engine::build_parachain;


#[derive(Args)]
pub struct BuildParachainCommand {
    #[arg(short = 'p', long = "path", help = "Directory ath for your project, [default: current directory]")]
    pub(crate) path: Option<PathBuf>,
}

impl BuildParachainCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        clear_screen()?;
        intro(format!(
            "{}: Building a parachain",
            style(" Pop CLI ").black().on_magenta()
        ))?;
        set_theme(Theme);
        build_parachain(&self.path)?;

        outro("Build Completed Successfully!")?;   
        Ok(())
    }
}