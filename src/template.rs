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
    let url = match template {
        EPT => "https://github.com/paritytech/extended-parachain-template.git",
    };
    Repository::clone(url, target)?;
    Ok(())
}
