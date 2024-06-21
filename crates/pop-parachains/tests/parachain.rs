// SPDX-License-Identifier: GPL-3.0
use anyhow::Result;
use pop_parachains::Zombienet;

const CONFIG_FILE_PATH: &str = "../../tests/networks/pop.toml";
const BINARY_VERSION: &str = "v1.13.0";
const RUNTIME_VERSION: &str = "v1.2.7";

#[tokio::test]
async fn test_spawn_polkadot_and_two_parachains() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		CONFIG_FILE_PATH,
		Some(BINARY_VERSION),
		Some(RUNTIME_VERSION),
		Some(BINARY_VERSION),
		Some(RUNTIME_VERSION),
		Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}
