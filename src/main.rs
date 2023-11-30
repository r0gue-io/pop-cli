mod cli;
mod generator;
mod pallet;
mod template;
mod helpers;

use cli::Cli;
use pallet::TemplatePalletConfig;
use std::path::Path;

use crate::template::Config;

fn main() -> anyhow::Result<()> {
    // eprintln!("DEBUG: Generator code is only used for development purposes");
    // generator::generate();
    // std::process::exit(0);

    let cli = <Cli as clap::Parser>::parse();
    match cli.intent {
        cli::Intention::Create(cli::TemplateCmd {
            name,
            template,
            symbol,
            decimals,
            initial_endowment,
        }) => {
            println!("Starting {} on `{}`!", template, name);
            let destination_path = Path::new(&name);
            template::instantiate_template_dir(
                &template,
                destination_path,
                Config {
                    symbol: symbol.expect("default values"),
                    decimals: decimals.expect("default values").parse::<u8>()?,
                    initial_endowment: initial_endowment.expect("default values"),
                },
            )?;
            println!("cd into `{name}` and enjoy hacking! 🚀");
        }
        cli::Intention::Pallet(cli::PalletCmd {
            name,
            authors,
            description,
            path
        }) => {
            pallet::create_pallet_template(path, TemplatePalletConfig {
                name,
                authors: authors.expect("default values"),
                description: description.expect("default values"),
            })?;
        }
    };

    Ok(())
}
