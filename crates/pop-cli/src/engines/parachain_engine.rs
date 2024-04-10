use crate::{
	commands::new::parachain::Template,
	helpers::{clone_and_degit, sanitize, write_to_file},
};
use anyhow::Result;
use duct::cmd;
use git2::Repository;
use std::{
	fs,
	path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub fn build_parachain(path: &Option<PathBuf>) -> anyhow::Result<()> {
	cmd("cargo", vec!["build", "--release"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}
