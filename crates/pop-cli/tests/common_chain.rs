// SPDX-License-Identifier: GPL-3.0
//! Integration tests for common chain helpers that require spawning a node.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use pop_cli::{cli::MockCli, common::chain::configure};
use pop_common::test_env::TestNode;
use url::Url;

#[tokio::test]
async fn configure_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let select_message = "Select a chain (type to filter)";
	let input_message = "Enter the URL of the chain:";
	let mut cli = MockCli::new()
		.expect_select(select_message.to_string(), Some(true), true, None, 1, None)
		.expect_input(input_message, node.ws_url().into());
	let chain =
		configure(select_message, input_message, node.ws_url(), &None, |_| true, &mut cli).await?;
	assert_eq!(chain.url, Url::parse(node.ws_url())?);
	cli.verify()
}
