// SPDX-License-Identifier: GPL-3.0

use pop_common::Profile;

use crate::{ContainerEngine, Error};
use srtool_lib::{get_image_digest, get_image_tag};
use std::{env, fs, path::PathBuf, process::Command};

pub mod container_engine;

const DEFAULT_IMAGE: &str = "docker.io/paritytech/srtool";
const ONE_HOUR: u64 = 60 * 60;

/// Builds and executes the command for running the deterministic runtime build process using
/// srtool.
pub struct SrToolBuilder {
	cache_mount: String,
	default_features: String,
	digest: String,
	engine: ContainerEngine,
	image: String,
	package: String,
	path: PathBuf,
	profile: Profile,
	runtime_dir: PathBuf,
	tag: String,
}

impl SrToolBuilder {
	/// Creates a new instance of `SrToolBuilder`.
	///
	/// # Arguments
	/// * `engine` - The container engine to use.
	/// * `path` - The path to the project.
	/// * `package` - The runtime package name.
	/// * `runtime_dir` - The directory path where the runtime is located.
	pub fn new(
		engine: ContainerEngine,
		path: Option<PathBuf>,
		package: String,
		runtime_dir: PathBuf,
	) -> Result<Self, Error> {
		let default_features = String::new();
		let profile = Profile::Release;
		let tag = get_image_tag(Some(ONE_HOUR)).map_err(|_| Error::ImageTagRetrievalFailed)?;
		let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
		let dir = fs::canonicalize(path.unwrap_or_else(|| PathBuf::from("./")))?;
		let tmpdir = env::temp_dir().join("cargo");

		let no_cache = engine == ContainerEngine::Podman;
		let cache_mount =
			if !no_cache { format!("-v {}:/cargo-home", tmpdir.display()) } else { String::new() };

		Ok(Self {
			cache_mount,
			default_features,
			digest,
			engine,
			image: DEFAULT_IMAGE.to_string(),
			package,
			path: dir,
			profile,
			runtime_dir,
			tag,
		})
	}
	/// Executes the runtime build process and returns the path of the generated `.wasm` file.
	pub fn generate_deterministic_runtime(&self) -> Result<PathBuf, Error> {
		let command = self.build_command();
		Command::new("sh").arg("-c").arg(command).spawn()?.wait_with_output()?;

		let wasm_path = self.get_runtime_path();
		Ok(wasm_path)
	}

	// Builds the srtool runtime container command string.
	fn build_command(&self) -> String {
		format!(
			"{} run --name srtool --rm \
			 -e PACKAGE={} \
			 -e RUNTIME_DIR={} \
			 -e DEFAULT_FEATURES={} \
			 -e PROFILE={} \
			 -e IMAGE={} \
			 -v {}:/build \
			 {} \
			 {}:{} build --app --json",
			self.engine,
			self.package,
			self.runtime_dir.display(),
			self.default_features,
			self.profile,
			self.digest,
			self.path.display(),
			self.cache_mount,
			self.image,
			self.tag
		)
	}
	// Returns the expected output path of the compiled runtime `.wasm` file.
	fn get_runtime_path(&self) -> PathBuf {
		self.runtime_dir
			.join("target")
			.join("srtool")
			.join(self.profile.to_string())
			.join("wbuild")
			.join(&self.package)
			.join(format!("{}.compact.compressed.wasm", self.package.replace("-", "_")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn srtool_builder_new_works() -> Result<()> {
		let srtool_builer = SrToolBuilder::new(
			ContainerEngine::Docker,
			None,
			"parachain-template-runtime".to_string(),
			PathBuf::from("./runtime"),
		)?;
		assert_eq!(
			srtool_builer.cache_mount,
			format!("-v {}:/cargo-home", env::temp_dir().join("cargo").display())
		);
		assert_eq!(srtool_builer.default_features, "");

		let tag = get_image_tag(Some(ONE_HOUR)).map_err(|_| Error::ImageTagRetrievalFailed)?;
		let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
		assert_eq!(srtool_builer.digest, digest);
		assert_eq!(srtool_builer.tag, tag);

		assert!(srtool_builer.engine == ContainerEngine::Docker);
		assert_eq!(srtool_builer.image, DEFAULT_IMAGE);
		assert_eq!(srtool_builer.package, "parachain-template-runtime");
		assert_eq!(srtool_builer.path, fs::canonicalize(PathBuf::from("./"))?);
		assert_eq!(srtool_builer.profile, Profile::Release);
		assert_eq!(srtool_builer.runtime_dir, PathBuf::from("./runtime"));

		Ok(())
	}

	#[test]
	fn build_command_works() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let tag = get_image_tag(Some(ONE_HOUR)).map_err(|_| Error::ImageTagRetrievalFailed)?;
		let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
		assert_eq!(
			SrToolBuilder::new(
				ContainerEngine::Podman,
				Some(path.to_path_buf()),
				"parachain-template-runtime".to_string(),
				PathBuf::from("./runtime"),
			)?
			.build_command(),
			format!(
				"podman run --name srtool --rm \
			 -e PACKAGE=parachain-template-runtime \
			 -e RUNTIME_DIR=./runtime \
			 -e DEFAULT_FEATURES= \
			 -e PROFILE=release \
			 -e IMAGE={} \
			 -v {}:/build \
			 {} \
			 {}:{} build --app --json",
				digest,
				fs::canonicalize(path)?.display(),
				String::new(),
				DEFAULT_IMAGE,
				tag
			)
		);
		Ok(())
	}

	#[test]
	fn get_runtime_path_works() -> Result<()> {
		assert_eq!(SrToolBuilder::new(
			ContainerEngine::Podman,
			None,
			"template-runtime".to_string(),
			PathBuf::from("./runtime-folder"),
		)?.get_runtime_path().display().to_string(), "./runtime-folder/target/srtool/release/wbuild/template-runtime/template_runtime.compact.compressed.wasm");
		Ok(())
	}
}
