// SPDX-License-Identifier: GPL-3.0
//! Integration tests for try-runtime helpers that require spawning a node.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use pop_chains::{
	set_up_client,
	state::{LiveState, State},
	try_runtime::TryStateSelect,
};
use pop_cli::{
	cli::MockCli,
	common::try_runtime::{
		DEFAULT_BLOCK_HASH, get_try_state_items, guide_user_to_select_try_state, update_live_state,
	},
};
use pop_common::test_env::TestNode;

#[derive(Default)]
struct MockCommand {
	state: Option<State>,
}

#[tokio::test]
async fn update_live_state_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let mut live_state = LiveState::default();
	let mut cmd = MockCommand::default();
	let mut cli = MockCli::new()
		.expect_input("Enter the live chain of your node:", node_url.to_string())
		.expect_input("Enter the block hash (optional):", DEFAULT_BLOCK_HASH.to_string());
	update_live_state(&mut cli, &mut live_state, &mut cmd.state)?;
	match cmd.state {
		Some(State::Live(ref live_state)) => {
			assert_eq!(live_state.uri, Some(node_url.to_string()));
		},
		_ => panic!("Expected live state"),
	}
	cli.verify()?;
	Ok(())
}

#[tokio::test]
async fn guide_user_to_select_try_state_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let client = set_up_client(node_url).await?;
	let pallets = pop_cli::common::chain::get_pallets(&client).await?;
	let pallet_items: Vec<(String, String)> =
		pallets.into_iter().map(|pallet| (pallet.name, pallet.docs)).collect();

	for (option, uri, expected) in [
		(0, None, TryStateSelect::None),
		(1, None, TryStateSelect::All),
		(2, None, TryStateSelect::RoundRobin(10)),
		(
			3,
			None,
			TryStateSelect::Only(
				["System", "Balances", "Proxy"].iter().map(|s| s.as_bytes().to_vec()).collect(),
			),
		),
		(3, Some(node_url.to_string()), TryStateSelect::Only(Vec::new())),
	] {
		let mut cli = MockCli::new().expect_select(
			"Select state tests to execute:",
			Some(true),
			true,
			Some(get_try_state_items()),
			option,
			None,
		);
		if let TryStateSelect::RoundRobin(..) = expected {
			cli = cli.expect_input("Enter the number of rounds:", "10".to_string());
		} else if let TryStateSelect::Only(..) = expected {
			if uri.is_some() {
				cli = cli.expect_multiselect(
					"Select pallets (select with SPACE):",
					Some(true),
					true,
					Some(pallet_items.clone()),
					Some(true),
				);
			} else {
				cli = cli.expect_input(
					"Enter the pallet names separated by commas:\nPallet names must be capitalized exactly as defined in the runtime.",
					"System, Balances, Proxy".to_string(),
				);
			}
		}
		let got = guide_user_to_select_try_state(&mut cli, uri).await?;
		if matches!(expected, TryStateSelect::Only(_)) && matches!(got, TryStateSelect::Only(_)) {
			cli.verify()?;
			continue;
		}
		assert_eq!(got, expected);
		cli.verify()?;
	}
	Ok(())
}
