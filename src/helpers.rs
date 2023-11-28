use anyhow::Result;
use std::{fs, path::Path};
use git2::Repository;
pub(crate) fn sanitize(target: &Path) -> Result<()> {
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

/// Clone `url` into `target` and degit it
pub(crate) fn clone_and_degit(url: &str, target: &Path) -> Result<()> {
    let repo = Repository::clone(url, target)?;
    let git_dir = repo.path();
    fs::remove_dir_all(&git_dir)?;
    Ok(())
}