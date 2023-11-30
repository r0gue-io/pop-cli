use anyhow::Result;
use git2::Repository;
use std::{
    env::current_dir,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
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

// TODO: Check the usage of `expect`. We don't want to leave the outdir in a unhygenic state
pub(crate) fn write_to_file<'a>(path: &Path, contents: &'a str) {
    println!("Writing to {}", path.display());
    use std::io::Write;
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    if path.extension().map_or(false, |ext| ext == "rs") {
        let output = std::process::Command::new("rustfmt")
            .arg(path.to_str().unwrap())
            .output()
            .expect("failed to execute rustfmt");
    
        if !output.status.success() {
            eprintln!("rustfmt exited with non-zero status code.");
        }
    }
}

/// Clone `url` into `target` and degit it
pub(crate) fn clone_and_degit(url: &str, target: &Path) -> Result<()> {
    let repo = Repository::clone(url, target)?;
    let git_dir = repo.path();
    fs::remove_dir_all(&git_dir)?;
    Ok(())
}

/// Resolve pallet path
/// For a template it should be `<template>/pallets/`
/// For no path, it should just place it in the current working directory
pub(crate) fn resolve_pallet_path(path: Option<String>) -> PathBuf {
    if let Some(path) = path {
        return Path::new(&path).to_path_buf();
    }
    // Check if inside a template
    let cwd = current_dir().expect("current dir is inaccessible");
    if cwd.join("runtime").exists() && cwd.join("node").exists() && cwd.join("pallets").exists() {
        Path::new("pallets").to_path_buf()
    } else {
        cwd
    }
}
