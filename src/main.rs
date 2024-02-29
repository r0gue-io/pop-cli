mod commands;
#[cfg(feature = "parachain")]
mod engines;
#[cfg(feature = "parachain")]
mod git;
#[cfg(feature = "parachain")]
mod helpers;
#[cfg(feature = "parachain")]
mod parachains;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use cliclack::ThemeState;
use console::{style, Style};
use std::{fs::create_dir_all, path::PathBuf};

#[derive(Parser)]
#[command(author, version, about, styles=get_styles())]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[command(subcommand_required = true)]
enum Commands {
    New(commands::new::NewArgs),
    /// Deploy a parachain or smart contract.
    Up(commands::up::UpArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::New(args) => match &args.command {
            #[cfg(feature = "parachain")]
            commands::new::NewCommands::Parachain(cmd) => cmd.execute(),
            #[cfg(feature = "parachain")]
            commands::new::NewCommands::Pallet(cmd) => cmd.execute(),
        },
        Commands::Up(args) => Ok(match &args.command {
            #[cfg(feature = "parachain")]
            commands::up::UpCommands::Parachain(cmd) => cmd.execute().await?,
        }),
    }
}

#[cfg(feature = "parachain")]
fn cache() -> Result<PathBuf> {
    let cache_path = dirs::cache_dir()
        .ok_or(anyhow!("the cache directory could not be determined"))?
        .join("pop");
    // Creates pop dir if needed
    create_dir_all(cache_path.as_path())?;
    Ok(cache_path)
}

pub fn get_styles() -> clap::builder::Styles {
    use clap::builder::styling::{AnsiColor, Color, Style};
    clap::builder::Styles::styled()
        .usage(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::BrightCyan))),
        )
        .header(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::BrightCyan))),
        )
        .literal(Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightMagenta))))
        .invalid(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Red))),
        )
        .error(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Red))),
        )
        .valid(
            Style::new()
                .bold()
                .underline()
                .fg_color(Some(Color::Ansi(AnsiColor::BrightMagenta))),
        )
        .placeholder(Style::new().fg_color(Some(Color::Ansi(AnsiColor::White))))
}

struct Theme;

impl cliclack::Theme for Theme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => Style::new().bright().magenta(),
            ThemeState::Error(_) => Style::new().bright().red(),
            _ => Style::new().magenta().dim(),
        }
    }

    fn state_symbol_color(&self, _state: &ThemeState) -> Style {
        Style::new().bright().magenta()
    }

    fn info_symbol(&self) -> String {
        "⚙".into()
    }
}
