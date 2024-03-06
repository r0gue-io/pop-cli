use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::engines::pallet_engine::{self, TemplatePalletConfig};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct AddArgs {
    #[command(subcommand)]
    /// Pallet to add to the runtime
    pub(crate) pallet: AddPallet,
    #[arg(global = true, short)]
    /// Runtime path;
    /// Cargo Manifest path will be inferred as `../Cargo.toml`
    pub(crate) runtime: Option<String>,
}

#[derive(Subcommand, Clone)]
#[command(subcommand_required = true)]
pub(crate) enum AddPallet {
    /// Insert `pallet-template` into the runtime.
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
            Some(ref s) => PathBuf::from(s),
            None => {
                // TODO: Fetch runtime either from cache
                // Fix: This is a placeholder path, should not be used
                PathBuf::from("my-app/runtime/src/lib.rs")
            }
        };
        let pallet = match self.pallet {
            AddPallet::Template => format!("pallet-template"),
            AddPallet::Frame(FrameArgs { ref name }) => {
                eprintln!("Sorry, frame pallets cannot be added right now");
                std::process::exit(1);
                // format!("FRAME-pallet-{name}")
            },
        };
        pallet_engine::execute(self.pallet.clone(), runtime_path.clone())?;
        println!("Added {}\n-> to {}", pallet, runtime_path.display());
        Ok(())
    }
}
