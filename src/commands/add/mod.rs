use std::path::PathBuf;

use crate::engines::pallet_engine;
use clap::{Args, Subcommand};
use cliclack::{intro, outro};
use console::style;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct AddArgs {
	#[command(subcommand)]
	/// Pallet to add to the runtime
	pub(crate) pallet: AddPallet,
	#[arg(global = true, short, long)]
	/// Runtime path; for example: `sub0/runtime/src/lib.rs`
	/// Cargo Manifest path will be inferred as `../Cargo.toml`
	pub(crate) runtime: Option<String>,
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
		let runtime_path = match self.runtime {
			Some(ref s) => {
				let path = PathBuf::from(s);
				if !path.exists() {
					anyhow::bail!("Invalid runtime path: {}", path.display());
				}
				path
			},
			None => {
				// TODO: Fetch runtime either from cache
				// Fix: This is a placeholder path, should not be used
				unimplemented!(
					"provide a runtime path until cache is implemented: --runtime <path>"
				);
			},
		};
		let pallet = match self.pallet {
			AddPallet::Template => format!("pallet-parachain-template"),
			AddPallet::Frame(FrameArgs { .. }) => {
				eprintln!("Sorry, frame pallets cannot be added right now");
				std::process::exit(1);
				// format!("FRAME-pallet-{name}")
			},
		};
		intro(format!(
			"{}: Adding pallet \"{}\"!",
			style(" Pop CLI ").black().on_magenta(),
			&pallet,
		))?;
		pallet_engine::execute(self.pallet.clone(), runtime_path.clone())?;
		outro(format!("Added {}\n-> to {}", pallet, runtime_path.display()))?;
		Ok(())
	}
}
