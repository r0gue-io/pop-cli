
use std::path::PathBuf;
use crate::{engines::pallet_engine, style::Theme};
use clap::Args;
use cliclack::{intro, outro, set_theme, clear_screen};
use console::style;

pub(crate) enum PalletType {
	/// `pallet-parachain-template`.
	Template,
	/// frame-pallet.
	Frame,
}
pub(crate) struct PalletInfo {
    pub(crate) name: String,
    pub(crate) pallet_type: PalletType
}


#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct AddPalletCommand {
    /// Name of the frame-pallet to insert into the runtime.
    #[clap(name = "frame", short('f'), long)]
    pub(crate) frame_pallet: Option<String>,
	/// Runtime path; for example: `sub0/runtime/src/lib.rs`
	/// Cargo Manifest path will be inferred as `../Cargo.toml`
	pub(crate) runtime: Option<String>,
}

impl AddPalletCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
        clear_screen()?;
		set_theme(Theme);
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
        let pallet = PalletInfo {
            name: format!("pallet-parachain-template"),
            pallet_type: PalletType::Template,
        };
        if self.frame_pallet.is_some() {
            eprintln!("Sorry, frame pallets cannot be added right now");
            std::process::exit(1);
            // pallet = PalletInfo {
            //     name: format!("FRAME-pallet-{name}"),
            //     pallet_type: PalletType::FRAME,
            // };
        }
		intro(format!(
			"{}: Adding pallet \"{}\"!",
			style(" Pop CLI ").black().on_magenta(),
			&pallet.name,
		))?;
		pallet_engine::execute(pallet, runtime_path.clone())?;
		outro(format!("Added {}\n-> to {}", &self.frame_pallet.clone().unwrap_or(format!("pallet-parachain-template")), runtime_path.display()))?;
		Ok(())
	}
}
