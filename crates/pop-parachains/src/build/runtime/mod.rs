// SPDX-License-Identifier: GPL-3.0

use pop_common::Profile;

use crate::{ContainerEngine, Error};
use srtool_lib::{get_image_digest, get_image_tag};
use std::{env, fs, path::PathBuf, process::Command};

pub mod container_engine;

const DEFAULT_IMAGE: &str = "docker.io/paritytech/srtool";
const ONE_HOUR: u64 = 60 * 60;

// Generates chain spec files for the parachain.
pub async fn generate_deterministic_runtime(
	engine: ContainerEngine,
	path: Option<PathBuf>,
	package: String,
	runtime_dir: PathBuf,
) -> Result<(), Error> {
	// format!("{engine} pull {image}:{tag}");
	let runtime = runtime_dir.display();
	let default_features = String::new();
	let profile = Profile::Release;
	let tag = get_image_tag(Some(ONE_HOUR)).map_err(|_| Error::ImageTagRetrievalFailed)?;
	let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
	let dir = fs::canonicalize(path.unwrap_or(PathBuf::from("./")))?;
	let runtime_dir = dir.display();
	let tmpdir = env::temp_dir().join("cargo");
	let no_cache = if engine == ContainerEngine::Podman { true } else { false };
	let cache_mount = if !no_cache {
		format!("-v {tmpdir}:/cargo-home", tmpdir = tmpdir.display())
	} else {
		String::new()
	};
	let command = format!(
		"{engine} run --name srtool --rm \
				-e PACKAGE={package} \
				-e RUNTIME_DIR={runtime} \
				-e DEFAULT_FEATURES={default_features} \
				-e PROFILE={profile} \
				-e IMAGE={digest} \
				-v {runtime_dir}:/build \
				{cache_mount} \
				{DEFAULT_IMAGE}:{tag} build  --app --json"
	);
	Command::new("sh").arg("-c").arg(command).spawn()?.wait_with_output()?;
	Ok(())
}
