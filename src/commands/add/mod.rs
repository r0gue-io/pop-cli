use std::path::PathBuf;

use crate::engines::pallet_engine;
use clap::{Args, Subcommand};
use cliclack::{intro, outro};
use console::style;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct AddArgs {
	#[command(subcommand)]
	commands: AddCommands,
	#[arg(global = true, short, long)]
	/// Runtime path; for example: `sub0/runtime/src/lib.rs`
	/// Runtime cargo manifest path will be inferred as `(parent of lib.rs)/Cargo.toml`
	pub(crate) runtime: Option<String>,
}
#[derive(Subcommand)]
#[command(subcommand_required = true)]
pub(crate) enum AddCommands {
	#[command(subcommand)]
	#[clap(alias = "p")]
	Pallet(AddPallet),
}

#[derive(Subcommand, Clone)]
#[command(subcommand_required = true)]
pub(crate) enum AddPallet {
	/// Insert `pallet-parachain-template` into the runtime.
	Template,
	/// Insert a frame-pallet into the runtime.
	Frame(FrameArgs),
}

#[derive(Args, Clone)]
pub(crate) struct FrameArgs {
	#[arg(short, long)]
	// TODO: Not ready for use
	pub(crate) name: String,
}

impl AddArgs {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		match self.commands {
			AddCommands::Pallet(ref cmd) => cmd.clone().execute(&self.runtime),
		}
	}
}
impl AddPallet {
	pub(crate) fn execute(self, runtime_path: &Option<String>) -> anyhow::Result<()> {
		let runtime_path = match runtime_path {
			Some(ref s) => {
				let path = PathBuf::from(s);
				if !path.exists() {
					anyhow::bail!("Invalid runtime path: {}", path.display());
				}
				path
			},
			None => {
				// TODO: Fetch runtime either from cache
				unimplemented!(
					"provide a runtime path until feat:cache is implemented: --runtime <path>"
				);
			},
		};
		let pallet = match self {
			AddPallet::Template => "pallet-parachain-template".to_string(),
			AddPallet::Frame(FrameArgs { .. }) => {
				eprintln!("Sorry, frame pallets cannot be added right now");
				std::process::exit(1);
				// format!("FRAME pallet-{name}")
			},
		};
		intro(format!(
			"{}: Adding pallet \"{}\"!",
			style(" Pop CLI ").black().on_magenta(),
			&pallet,
		))?;
		pallet_engine::execute(self, runtime_path.clone())?;
		outro(format!("Added {}\n-> to {}", pallet, runtime_path.display()))?;
		Ok(())
	}
}
