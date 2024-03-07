use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "parachain")]
pub mod pallet;
#[cfg(feature = "parachain")]
pub mod parachain;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct NewArgs {
	#[command(subcommand)]
	pub command: NewCommands,
}

#[derive(Subcommand)]
pub enum NewCommands {
	/// Generate a new parachain template
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(parachain::NewParachainCommand),
	/// Generate a new pallet template
	#[cfg(feature = "parachain")]
	#[clap(alias = "m")] // (m)odule, as p used above
	Pallet(pallet::NewPalletCommand),
	/// Generate a new smart contract template
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::NewContractCommand),
}
