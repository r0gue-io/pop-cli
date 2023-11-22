use crate::cli::Template;
use anyhow::Result;
use git2::Repository;
use std::{fs, path::Path};
use walkdir::WalkDir;

pub struct Config;

pub fn instantiate_template_dir(
    template: &Template,
    target: &Path,
    // config: &Config,
) -> Result<()> {
    use Template::*;
    // TODO : if target folder exists, prompt user to clean dir or abort
    let url = match template {
        EPT => "https://github.com/paritytech/extended-parachain-template.git",
        FPT => "https://github.com/paritytech/frontier-parachain-template.git",
        Contracts => "https://github.com/paritytech/substrate-contracts-node.git",
        Vanilla => {
            return instantiate_vanilla_template(target, Some(Config));
        }
    };
    Repository::clone(url, target)?;
    Ok(())
}
// TODO: The config will shape the emitted template
pub fn instantiate_vanilla_template(target: &Path, config: Option<Config>) -> Result<()> {
    let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
    let temp_path = temp_dir.path();
    println!("Temporary directory created at {:?}", temp_path);

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

    Ok(())
}
