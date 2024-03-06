use clap::{Args, Subcommand};

pub mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
    #[command(subcommand)]
    pub command: TestCommands,
}

#[derive(Subcommand)]
pub(crate) enum TestCommands {
    /// Test the contract
    #[cfg(feature = "contract")]
    Contract(contract::TestContractCommand),
}