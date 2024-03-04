use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::engines::pallet_engine::TemplatePalletConfig;

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

#[derive(Subcommand)]
#[command(subcommand_required = true)]
pub(crate) enum AddPallet {
    /// Insert `pallet-template` into the runtime. Useful for quick start pallet-template dev
    Template(TemplatePalletConfig),
    /// Insert a frame-pallet into the runtime.
    Frame(FrameArgs),
}

#[derive(Args)]
pub(crate) struct FrameArgs {
    #[arg(short, long)]
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
            AddPallet::Template(TemplatePalletConfig {
                ref name,
                ref authors,
                ref description,
            }) => format!(
                "Template with name: {name}, authors: {authors:?}, description: {description:?}"
            ),
            AddPallet::Frame(FrameArgs { ref name }) => format!("p-frame-{name}"),
        };
        crate::engines::pallet_engine::execute(self.pallet.clone(), runtime_path)?;
        println!("Added {}\n-> to {}", pallet, runtime_path.display());
        Ok(())
    }
}
