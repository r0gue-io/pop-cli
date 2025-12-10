// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use duct::cmd;
use pop_common::{Docker, Profile, manifest::from_path};
use std::{
	env, fs,
	path::{Path, PathBuf},
};

const DEFAULT_IMAGE: &str = "docker.io/paritytech/srtool";
const SRTOOL_TAG_URL: &str =
	"https://raw.githubusercontent.com/paritytech/srtool/master/RUSTC_VERSION";

/// Builds and executes the command for running a deterministic runtime build process using
/// srtool.
pub struct DeterministicBuilder {
	/// Mount point for cargo cache.
	cache_mount: String,
	/// List of default features to enable during the build process.
	default_features: String,
	/// Digest of the image for reproducibility.
	digest: String,
	/// Name of the image used for building.
	image: String,
	/// The runtime package name.
	package: String,
	/// The path to the project directory.
	path: PathBuf,
	/// The profile used for building.
	profile: Profile,
	/// The directory path where the runtime is located.
	runtime_dir: PathBuf,
	/// The tag of the image to use.
	tag: String,
}

impl DeterministicBuilder {
	/// Creates a new instance of `Builder`.
	///
	/// # Arguments
	/// * `path` - The path to the project.
	/// * `package` - The runtime package name.
	/// * `profile` - The profile to build the runtime.
	/// * `runtime_dir` - The directory path where the runtime is located.
	/// * `tag` - The tag of the srtool image to be used
	pub async fn new(
		path: Option<PathBuf>,
		package: &str,
		profile: Profile,
		runtime_dir: PathBuf,
		tag: Option<String>,
	) -> Result<Self, Error> {
		let default_features = String::new();
		let tag = match tag {
			Some(tag) => tag,
			_ => pop_common::docker::fetch_image_tag(SRTOOL_TAG_URL).await?,
		};
		let digest = Docker::get_image_digest(DEFAULT_IMAGE, &tag)?;
		let dir = fs::canonicalize(path.unwrap_or_else(|| PathBuf::from("./")))?;
		let tmpdir = env::temp_dir().join("cargo");

		let cache_mount = format!("{}:/cargo-home", tmpdir.display());

		Ok(Self {
			cache_mount,
			default_features,
			digest,
			image: DEFAULT_IMAGE.to_string(),
			package: package.to_owned(),
			path: dir,
			profile,
			runtime_dir,
			tag,
		})
	}

	/// Executes the runtime build process and returns the path of the generated file.
	pub fn build(&self) -> Result<PathBuf, Error> {
		let args = self.build_args()?;
		cmd("docker", args).stdout_null().stderr_null().run()?;
		Ok(self.get_output_path())
	}

	// Builds the srtool runtime container command string.
	fn build_args(&self) -> Result<Vec<String>, Error> {
		let package = format!("PACKAGE={}", self.package);
		// Runtime dir might be the absolute path to the runtime dir if the user didn't specified
		// it. This causes docker runs to fail, so we need to strip the prefix.
		let absolute_workspace_path = std::fs::canonicalize(
			rustilities::manifest::find_workspace_manifest(std::env::current_dir()?)
				.ok_or(anyhow::anyhow!("Pop cannot determine your workspace path"))?
				.parent()
				.expect("A workspace manifest is a file and hence always have a parent; qed;"),
		)?;
		let runtime_dir = self
			.runtime_dir
			.strip_prefix(absolute_workspace_path)
			.unwrap_or(&self.runtime_dir);
		let runtime_dir = format!("RUNTIME_DIR={}", runtime_dir.display());
		let default_features = format!("DEFAULT_FEATURES={}", self.default_features);
		let profile = match self.profile {
			Profile::Debug => "PROFILE=dev".to_owned(),
			_ => format!("PROFILE={}", self.profile),
		};
		let image_digest = format!("IMAGE={}", self.digest);
		let volume = format!("{}:/build", self.path.display());
		let image_tag = format!("{}:{}", self.image, self.tag);

		let args = vec![
			"run".to_owned(),
			"--name".to_owned(),
			"srtool".to_owned(),
			"--rm".to_owned(),
			"-e".to_owned(),
			package,
			"-e".to_owned(),
			runtime_dir,
			"-e".to_owned(),
			default_features,
			"-e".to_owned(),
			profile,
			"-e".to_owned(),
			image_digest,
			"-v".to_owned(),
			volume,
			"-v".to_owned(),
			self.cache_mount.clone(),
			image_tag,
			"build".to_owned(),
			"--app".to_owned(),
			"--json".to_owned(),
		];

		Ok(args)
	}

	// Returns the expected output path of the compiled runtime `.wasm` file.
	fn get_output_path(&self) -> PathBuf {
		let output_wasm = match self.profile {
			Profile::Debug => "wasm",
			_ => "compact.compressed.wasm",
		};
		self.runtime_dir
			.join("target")
			.join("srtool")
			.join(self.profile.to_string())
			.join("wbuild")
			.join(&self.package)
			.join(format!("{}.{}", self.package.replace("-", "_"), output_wasm))
	}
}

/// Determines whether the manifest at the supplied path is a supported Substrate runtime project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn is_supported(path: &Path) -> bool {
	let manifest = match from_path(path) {
		Ok(m) => m,
		Err(_) => return false,
	};
	// Simply check for a parachain dependency
	const DEPENDENCIES: [&str; 3] = ["frame-system", "frame-support", "substrate-wasm-builder"];
	let has_dependencies = DEPENDENCIES.into_iter().any(|d| {
		manifest.dependencies.contains_key(d) ||
			manifest.workspace.as_ref().is_some_and(|w| w.dependencies.contains_key(d))
	});
	let has_features = manifest.features.contains_key("runtime-benchmarks") ||
		manifest.features.contains_key("try-runtime");
	has_dependencies && has_features
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use fs::write;
	use pop_common::manifest::Dependency;
	use tempfile::tempdir;

	const SRTOOL_TAG: &str = "1.88.0";
	const SRTOOL_DIGEST: &str =
		"sha256:9902e50293f55fa34bc8d83916aad3fdf9ab3c74f2c0faee6dec8cc705a3a5d7";

	#[tokio::test]
	async fn srtool_builder_new_works() {
		Docker::ensure_running().unwrap();
		let srtool_builder = DeterministicBuilder::new(
			None,
			"parachain-template-runtime",
			Profile::Release,
			PathBuf::from("./runtime"),
			Some(SRTOOL_TAG.to_owned()),
		)
		.await
		.unwrap();
		assert_eq!(
			srtool_builder.cache_mount,
			format!("{}:/cargo-home", env::temp_dir().join("cargo").display())
		);
		assert_eq!(srtool_builder.default_features, "");
		assert_eq!(srtool_builder.digest, SRTOOL_DIGEST);
		assert_eq!(srtool_builder.tag, SRTOOL_TAG);

		assert_eq!(srtool_builder.image, DEFAULT_IMAGE);
		assert_eq!(srtool_builder.package, "parachain-template-runtime");
		assert_eq!(srtool_builder.path, fs::canonicalize(PathBuf::from("./")).unwrap());
		assert_eq!(srtool_builder.profile, Profile::Release);
		assert_eq!(srtool_builder.runtime_dir, PathBuf::from("./runtime"));
	}

	#[tokio::test]
	async fn build_args_works() {
		Docker::ensure_running().unwrap();

		let temp_dir = tempdir().unwrap();
		let path = temp_dir.path();
		assert_eq!(
			DeterministicBuilder::new(
				Some(path.to_path_buf()),
				"parachain-template-runtime",
				Profile::Production,
				PathBuf::from("./runtime"),
				Some(SRTOOL_TAG.to_owned())
			)
			.await
			.unwrap()
			.build_args()
			.unwrap(),
			vec!(
				"run",
				"--name",
				"srtool",
				"--rm",
				"-e",
				"PACKAGE=parachain-template-runtime",
				"-e",
				"RUNTIME_DIR=./runtime",
				"-e",
				"DEFAULT_FEATURES=",
				"-e",
				"PROFILE=production",
				"-e",
				&format!("IMAGE={SRTOOL_DIGEST}"),
				"-v",
				&format!("{}:/build", fs::canonicalize(path).unwrap().display()),
				"-v",
				&format!("{}:/cargo-home", env::temp_dir().join("cargo").display()),
				&format!("{DEFAULT_IMAGE}:{SRTOOL_TAG}"),
				"build",
				"--app",
				"--json"
			),
		);
	}

	#[tokio::test]
	async fn get_output_path_works() -> Result<()> {
		Docker::ensure_running()?;
		let srtool_builder = DeterministicBuilder::new(
			None,
			"template-runtime",
			Profile::Debug,
			PathBuf::from("./runtime-folder"),
			None,
		)
		.await?;
		assert_eq!(
			srtool_builder.get_output_path().display().to_string(),
			"./runtime-folder/target/srtool/debug/wbuild/template-runtime/template_runtime.wasm"
		);
		Ok(())
	}

	#[test]
	fn is_supported_works() -> Result<()> {
		let temp_dir = tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(path).run()?;
		assert!(!is_supported(&path.join(name)));

		// Parachain runtime with dependency
		let mut manifest = from_path(&path.join(name))?;
		manifest
			.dependencies
			.insert("substrate-wasm-builder".into(), Dependency::Simple("^0.14.0".into()));
		manifest.features.insert("try-runtime".into(), vec![]);
		let manifest = toml_edit::ser::to_string_pretty(&manifest)?;
		write(path.join(name).join("Cargo.toml"), manifest)?;
		assert!(is_supported(&path.join(name)));
		Ok(())
	}
}
