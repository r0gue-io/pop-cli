// SPDX-License-Identifier: GPL-3.0

use crate::{cli::traits::Cli, common::builds::PopComposeBuildArgs};
use anyhow::{Context, Result};
use clap::Args;
use pop_contracts::{
	VerifyContract
};
use regex::Regex;
use serde::Serialize;
use std::{fs::File, path::PathBuf};

#[derive(Args, Serialize)]
pub(crate) struct VerifyCommand {
	/// Directory path with flag for your project manifest [default: current directory manifest if exists]
	#[clap(short, long)]
	manifest_path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory manifest if exists]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "manifest_path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The reference `.contract` file (`*.contract`) that the selected
	/// contract will be checked against.
	#[clap(short, long)]
	contract_path: PathBuf,
}

impl VerifyCommand {
	pub(crate) fn execute(&self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Start verifying your contract, this make take a bit ⏳")?;

		let project_path =
			crate::common::builds::ensure_project_path(self.manifest_path.clone(), self.path_pos.clone());

		<VerifyContract<PopComposeBuildArgs>>::new_local(project_path, self.contract_path.clone()).execute()?;

		let _ = cli.success("The contract verification completed successfully ✅");

		Ok(())
	}}