// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{
	clear_screen, intro,
	log::{success, warning},
	outro, set_theme,
};
use pop_parachains::{build_parachain, generate_chain_spec, node_release_path};
use std::path::PathBuf;

#[derive(Args)]
pub struct BuildParachainCommand {
	#[arg(
		short = 'p',
		long = "path",
		help = "Directory path for your project, [default: current directory]"
	)]
	pub(crate) path: Option<PathBuf>,
	#[arg(
		short = 'i',
		long = "para_id",
		help = "Parachain id to be used when generating the chain spec files."
	)]
	pub(crate) para_id: Option<u32>,
}

impl BuildParachainCommand {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building your parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);
		warning("NOTE: this may take some time...")?;
		build_parachain(&self.path)?;

		success("Build Completed Successfully!")?;
		let release_path = node_release_path(&self.path)?;
		let mut generated_files = vec![format!("Binary generated at: \"{release_path}\"")];
		// If a para_id is provided, generate the chain spec
		if let Some(para_id) = self.para_id {
			let chain_spec = generate_chain_spec(release_path, &self.path, para_id)?;
			generated_files
				.push(format!("New raw chain specification file generated at: {chain_spec}"))
		}
		let generated_files: Vec<_> = generated_files
			.iter()
			.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
			.collect();
		success(format!("Generated files:\n{}", generated_files.join("\n")))?;
		outro(format!(
			"Need help? Learn more at {}\n",
			style("https://learn.onpop.io").magenta().underlined()
		))?;
		Ok(())
	}
}
