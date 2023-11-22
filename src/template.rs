use crate::{
    cli::Template,
    generator::{write_to_file, ChainSpec},
};
use anyhow::Result;
use git2::Repository;
use std::{fs, path::Path};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) symbol: String,
    pub(crate) decimals: u8,
    pub(crate) initial_endowment: String,
}

pub fn instantiate_template_dir(template: &Template, target: &Path, config: Config) -> Result<()> {
    use Template::*;
    // TODO : if target folder exists, prompt user to clean dir or abort
    sanitize(target)?;
    let url = match template {
        EPT => "https://github.com/paritytech/extended-parachain-template.git",
        FPT => "https://github.com/paritytech/frontier-parachain-template.git",
        Contracts => "https://github.com/paritytech/substrate-contracts-node.git",
        Vanilla => {
            return instantiate_vanilla_template(target, config);
        }
    };
    Repository::clone(url, target)?;
    Ok(())
}
// TODO: The config will shape the emitted template
pub fn instantiate_vanilla_template(target: &Path, config: Config) -> Result<()> {
    let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
    let temp_path = temp_dir.path();
    // println!("Temporary directory created at {:?}", temp_path);

    Repository::clone("https://github.com/weezy20/DoTemplate.git", temp_path)?;
    let source = temp_path.join("templates/vanilla-parachain");

    for entry in WalkDir::new(&source) {
        let entry = entry?;

        let source_path = entry.path();
        let destination_path = target.join(source_path.strip_prefix(&source)?);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination_path)?;
        } else {
            fs::copy(source_path, &destination_path)?;
        }
    }
    let chainspec = ChainSpec {
        token_symbol: config.symbol,
        decimals: config.decimals,
        initial_endowment: config.initial_endowment,
    };
    use askama::Template;
    write_to_file(
        &target.join("node/src/chain_spec.rs"),
        chainspec.render().expect("infallible").as_ref(),
    );

    Ok(())
}

fn sanitize(target: &Path) -> Result<()> {
    use std::io::{stdin, stdout, Write};
    if target.exists() {
        print!(
            "\"{}\" folder exists. Do you want to clean it? [y/n]: ",
            target.display()
        );
        stdout().flush()?;

        let mut input = String::new();
        stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() == "y" {
            fs::remove_dir_all(target)?;
        } else {
            return Err(anyhow::anyhow!(
                "User aborted due to existing target folder."
            ));
        }
    }
    Ok(())
}
