mod cli;
mod generator;
mod template;

use cli::Cli;
use std::path::Path;

use crate::template::Config;

fn main() -> anyhow::Result<()> {
    // eprintln!("DEBUG: Generator code is only used for development purposes");
    // generator::generate();
    // std::process::exit(0);

    let cli = <Cli as clap::Parser>::parse();
    let (app_name, template, config) = match cli.create {
        cli::Create::Create(cli::TemplateCmd {
            name,
            template,
            symbol,
            decimals,
            initial_endowment,
        }) => (
            name,
            template,
            Config {
                symbol: symbol.expect("default values"),
                decimals: decimals.expect("default values").parse::<u8>()?,
                initial_endowment: initial_endowment.expect("default values"),
            },
        ),
    };
    println!("Starting {} on `{}`!", template, app_name);
    let destination_path = Path::new(&app_name);
    template::instantiate_template_dir(&template, destination_path, config)?;
    println!("cd into `{app_name}` and enjoy hacking! 🚀");

    Ok(())
}
