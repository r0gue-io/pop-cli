// SPDX-License-Identifier: GPL-3.0
use anyhow::Result;
use pop_parachains::{Binary, Source, Zombienet};
use std::path::PathBuf;
use url::Url;

const CONFIG_FILE_PATH: &str = "../../tests/networks/pop.toml";
const TESTING_POLKADOT_VERSION: &str = "v1.12.0";
const POLKADOT_BINARY: &str = "polkadot-v1.12.0";
const POLKADOT_SDK: &str = "https://github.com/paritytech/polkadot-sdk";

#[tokio::test]
async fn test_spawn_polkadot_and_two_parachains() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		cache.clone(),
		CONFIG_FILE_PATH,
		Some(&TESTING_POLKADOT_VERSION.to_string()),
		Some(&TESTING_POLKADOT_VERSION.to_string()),
		Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
	)
	.await?;

	let working_dir = cache.join(".src");
	for binary in zombienet.missing_binaries() {
		binary.source(&working_dir, (), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[tokio::test]
async fn test_process_git() -> Result<()> {
	let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
	let cache = PathBuf::from(temp_dir.path());

	let version = TESTING_POLKADOT_VERSION;
	let repo = Url::parse(POLKADOT_SDK).expect("repository url valid");
	let source = Source::Git {
		url: repo.into(),
		reference: Some(format!("release-polkadot-{version}")),
		package: "polkadot".to_string(),
		artifacts: ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"]
			.iter()
			.map(|a| (a.to_string(), cache.join(format!("{a}-{version}"))))
			.collect(),
	};
	let binary =
		Binary::new("polkadot", version, cache.join(format!("polkadot-{version}")), source);
	binary.source(&cache, (), true).await?;
	assert!(temp_dir.path().join(POLKADOT_BINARY).exists());

	Ok(())
}
