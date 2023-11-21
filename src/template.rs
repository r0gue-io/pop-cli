use crate::cli::Template;
use anyhow::Result;
use git2::Repository;
use std::path::Path;

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
    };
    Repository::clone(url, target)?;
    Ok(())
}
