// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{
	clear_screen, intro,
	log::{success, warning},
	outro, set_theme,
};
use pop_parachains::{
	binary_path, build_parachain, export_wasm_file, generate_chain_spec,
	generate_genesis_state_file, generate_raw_chain_spec,
};
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
		long = "id",
		help = "Parachain ID to be used when generating the chain spec files."
	)]
	pub(crate) id: Option<u32>,
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
		let binary = binary_path(self.path.as_deref())?;
		let mut generated_files = vec![format!("Binary generated at: \"{}\"", binary.display())];
		// If para_id is provided, generate the chain spec
		if let Some(para_id) = self.id {
			let chain_spec = generate_chain_spec(self.path.as_deref(), para_id)?;
			generated_files.push(format!(
				"Plain text chain specification file generated at: {}",
				chain_spec.display()
			));
			let raw_chain_spec = generate_raw_chain_spec(self.path.as_deref())?;
			generated_files.push(format!(
				"New raw chain specification file generated at: {}",
				chain_spec.display()
			));
			let wasm_file = export_wasm_file(&raw_chain_spec, self.path.as_deref(), para_id)?;
			generated_files.push(format!(
				"WebAssembly runtime file exported at: {}",
				wasm_file.display().to_string()
			));
			let genesis_state_file =
				generate_genesis_state_file(&raw_chain_spec, self.path.as_deref(), para_id)?;
			generated_files.push(format!(
				"Genesis State exported at {} file",
				genesis_state_file.display().to_string()
			));
			console::Term::stderr().clear_last_lines(5)?;
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
