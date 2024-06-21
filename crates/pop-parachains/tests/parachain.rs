// SPDX-License-Identifier: GPL-3.0
use anyhow::Result;
use pop_parachains::Zombienet;

const CONFIG_FILE_PATH: &str = "../../tests/networks/pop.toml";
const TESTING_POLKADOT_VERSION: &str = "v1.12.0";

#[tokio::test]
async fn test_spawn_polkadot_and_two_parachains() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		CONFIG_FILE_PATH,
		Some(&TESTING_POLKADOT_VERSION.to_string()),
		Some(&TESTING_POLKADOT_VERSION.to_string()),
		Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}
