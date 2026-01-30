// SPDX-License-Identifier: GPL-3.0

//! Integration tests for pop-chains functionality.

#![cfg(feature = "integration-tests")]

use anyhow::Result;
use pop_chains::{generate_pallet_benchmarks, up::Zombienet, utils::helpers::get_preset_names};
use std::path::Path;

const BINARY_VERSION: &str = "stable2512";

#[tokio::test]
async fn launch_kusama() -> Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let cache = temp_dir.path().to_path_buf();

	let mut zombienet = Zombienet::new(
		&cache,
		Path::new("../../tests/networks/kusama.toml").try_into()?,
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
		Path::new("../../tests/networks/paseo.toml").try_into()?,
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
		Path::new("../../tests/networks/polkadot.toml").try_into()?,
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
		Path::new("../../tests/networks/polkadot+collectives.toml").try_into()?,
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
		Path::new("../../tests/networks/paseo+coretime.toml").try_into()?,
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
		Path::new("../../tests/networks/pop.toml").try_into()?,
		Some(BINARY_VERSION),
		None,
		Some(BINARY_VERSION),
		None,
		Some(&vec!["https://github.com/r0gue-io/pop-node#node-v0.3.0".to_string()]),
	)
	.await?;

	for binary in zombienet.binaries().filter(|b| !b.exists()) {
		binary.source(true, &(), true).await?;
	}

	zombienet.spawn().await?;
	Ok(())
}

#[test]
fn generate_pallet_benchmarks_works() -> Result<()> {
	let binary_path = "../../tests/runtimes/base_parachain_benchmark.wasm";
	generate_pallet_benchmarks(vec![
		"--pallet=pallet_timestamp".to_string(),
		"--extrinsic=*".to_string(),
		"--runtime".to_string(),
		binary_path.to_string(),
	])?;
	Ok(())
}

#[test]
fn get_preset_names_works() -> Result<()> {
	assert_eq!(
		get_preset_names(
			&std::env::current_dir()
				.unwrap()
				.join("../../tests/runtimes/base_parachain_benchmark.wasm")
				.canonicalize()?
		)?,
		vec!["development", "local_testnet"]
	);
	Ok(())
}
