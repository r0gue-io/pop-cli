// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use pop_parachains::Zombienet;

const BINARY_VERSION: &str = "v1.13.0";

#[tokio::test]
async fn launch_kusama() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		"../../tests/networks/kusama.toml",
		Some(BINARY_VERSION),
		Some("v1.2.7"),
		None,
		None,
		None,
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[tokio::test]
async fn launch_paseo() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		"../../tests/networks/paseo.toml",
		Some(BINARY_VERSION),
		Some("v1.2.4"),
		None,
		None,
		None,
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[tokio::test]
async fn launch_polkadot() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		"../../tests/networks/polkadot.toml",
		Some(BINARY_VERSION),
		Some("v1.2.7"),
		None,
		None,
		None,
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[tokio::test]
async fn launch_polkadot_and_system_parachain() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		"../../tests/networks/polkadot+collectives.toml",
		Some(BINARY_VERSION),
		Some("v1.2.7"),
		Some(BINARY_VERSION),
		Some("v1.2.7"),
		None,
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[tokio::test]
async fn launch_paseo_and_system_parachain() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		"../../tests/networks/paseo+coretime.toml",
		Some(BINARY_VERSION),
		None,
		Some(BINARY_VERSION),
		Some("v1.3.3"), // 1.3.3 is where coretime-paseo-local was introduced.
		None,
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[tokio::test]
async fn launch_paseo_and_two_parachains() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		"../../tests/networks/pop.toml",
		Some(BINARY_VERSION),
		None,
		Some(BINARY_VERSION),
		None,
		Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}
