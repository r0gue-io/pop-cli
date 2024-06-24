// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{
	clear_screen, intro,
	log::{success, warning},
	outro, set_theme,
};
use pop_parachains::{build_parachain, node_release_path};
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
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Building your parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		warning("NOTE: this may take some time...")?;
		build_parachain(&self.path)?;

		success("Build Completed Successfully!")?;

		let release_path = node_release_path(&self.path)?;
		// add next steps
		let mut next_steps = vec![format!("Binary generated in \"{release_path}\"")];
		// if let Some(network_config) = template.network_config() {
		// 	next_steps.push(format!(
		// 	"Use `pop up parachain -f {network_config}` to launch your parachain on a local network."
		// ))
		// }
		let next_steps: Vec<_> = next_steps
			.iter()
			.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
			.collect();
		success(format!("Next Steps:\n{}", next_steps.join("\n")))?;
		outro(format!(
			"Need help? Learn more at {}\n",
			style("https://learn.onpop.io").magenta().underlined()
		))?;
		Ok(())
	}
}
