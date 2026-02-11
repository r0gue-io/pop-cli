// SPDX-License-Identifier: GPL-3.0
//! Integration tests for up chain registration flows that require spawning a node.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use pop_cli::{
	cli::MockCli,
	commands::up::chain::{Deployment, MOCK_PROXIED_ADDRESS, UpCommand, create_temp_genesis_files},
};
use pop_common::test_env::TestNode;
use url::Url;

#[tokio::test]
async fn prepare_for_registration_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let mut cli = MockCli::new()
		.expect_select(
			"Select a chain (type to filter)".to_string(),
			Some(true),
			true,
			None,
			1,
			None,
		)
		.expect_input("Enter the relay chain node URL", node_url.into());
	let (genesis_state, genesis_code) = create_temp_genesis_files()?;
	let chain_config = UpCommand {
		id: Some(2000),
		genesis_state: Some(genesis_state.clone()),
		genesis_code: Some(genesis_code.clone()),
		proxied_address: Some(MOCK_PROXIED_ADDRESS.to_string()),
		..Default::default()
	}
	.prepare_for_registration(&mut Deployment::default(), false, &mut cli)
	.await?;

	assert_eq!(chain_config.id, 2000);
	assert_eq!(chain_config.genesis_artifacts.genesis_code_file, Some(genesis_code));
	assert_eq!(chain_config.genesis_artifacts.genesis_state_file, Some(genesis_state));
	assert_eq!(chain_config.chain.url, Url::parse(node_url)?);
	assert_eq!(chain_config.proxy, Some(format!("Id({})", MOCK_PROXIED_ADDRESS)));
	cli.verify()
}

#[tokio::test]
async fn register_fails_wrong_chain() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let mut cli = MockCli::new()
		.expect_intro("Deploy a chain")
		.expect_select("Select your deployment method:", Some(false), true, None, 3, None)
		.expect_info(format!(
			"You will need to sign a transaction to register on {}, using the `Registrar::register` function.",
			Url::parse(node_url)?.as_str()
		))
		.expect_outro_cancel(
			"Failed to find the pallet: Registrar\nRetry registration without reserve or rebuilding the chain specs using: `pop up --id 2000 --skip-registration`",
		);
	let (genesis_state, genesis_code) = create_temp_genesis_files()?;
	UpCommand {
		id: Some(2000),
		genesis_state: Some(genesis_state),
		genesis_code: Some(genesis_code),
		relay_chain_url: Some(Url::parse(node_url)?),
		path: std::path::PathBuf::from("./"),
		proxied_address: None,
		..Default::default()
	}
	.execute(&mut cli)
	.await?;
	Ok(())
}

#[tokio::test]
async fn reserve_id_fails_wrong_chain() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let mut cli = MockCli::new()
		.expect_intro("Deploy a chain")
		.expect_select("Select your deployment method:", Some(false), true, None, 3, None)
		.expect_info(format!(
			"You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.",
			Url::parse(node_url)?.as_str()
		))
		.expect_outro_cancel("Failed to find the pallet: Registrar");
	let (genesis_state, genesis_code) = create_temp_genesis_files()?;
	UpCommand {
		id: None,
		genesis_state: Some(genesis_state),
		genesis_code: Some(genesis_code),
		relay_chain_url: Some(Url::parse(node_url)?),
		path: std::path::PathBuf::from("./"),
		proxied_address: None,
		..Default::default()
	}
	.execute(&mut cli)
	.await?;
	Ok(())
}
