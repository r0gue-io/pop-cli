mod commands;
mod engines;
mod helpers;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[command(subcommand_required = true)]
pub enum Commands {
    New(commands::new::NewArgs),
}


fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _  = match &cli.command {
        Commands::New(args) => match &args.command {
            commands::new::NewCommands::Parachain(cmd) => cmd.execute(),
            commands::new::NewCommands::Pallet(cmd) => cmd.execute(),
        },
    };
    Ok(())
}
