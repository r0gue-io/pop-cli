// mod cli;
mod generator;
mod engines;
mod helpers;
mod commands;

use clap::{Args, Parser, Subcommand};
use std::path::Path;

#[derive(Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Here goes new, build, test, add, up, update, install, bench
#[derive(Subcommand)]
#[command(subcommand_required = true)]
pub enum Commands {
    New(NewArgs),
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct NewArgs {
    #[command(subcommand)]
    command: NewCommands,
}

#[derive(Subcommand)]
pub enum NewCommands {
    /// Generate a new parachain template
    Parachain(commands::new::parachain::NewParachainCommand),
     /// Generate a new pallet template
    Pallet(commands::new::pallet::NewPalletCommand),
}


fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::New(args) => match &args.command {
            NewCommands::Parachain(cmd) => cmd.execute(),
            NewCommands::Pallet(cmd) => cmd.execute(),
        },
    };
    Ok(())
}
