// SPDX-License-Identifier: GPL-3.0
//! Integration tests for call chain commands that require spawning a node.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use pop_chains::{CallItem, Function, parse_chain_metadata, set_up_client, supported_actions};
use pop_cli::{
	cli::MockCli,
	commands::call::chain::{Call, CallChainCommand, show_pallet},
	common::{chain, chain::Chain, wallet::USE_WALLET_PROMPT},
};
use pop_common::test_env::TestNode;
use scale_value::ValueDef;
use url::Url;

const BOB_SURI: &str = "//Bob";

#[tokio::test]
async fn guide_user_to_call_chain_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let mut call_config =
		CallChainCommand { pallet: Some("System".to_string()), sudo: true, ..Default::default() };

	let mut cli = MockCli::new()
		.expect_select(
			"Select a chain (type to filter)".to_string(),
			Some(true),
			true,
			None,
			1,
			None,
		)
		.expect_input("Which chain would you like to interact with?", node_url.into())
		.expect_select(
			"Select the function to call (type to filter)",
			Some(true),
			true,
			None,
			5,
			None,
		)
		.expect_input(
			"The value for `remark` might be too large to enter. You may enter the path to a file instead.",
			"0x11".into(),
		)
		.expect_confirm(
			"Are you sure you want to dispatch this function call with `Root` origin?",
			true,
		)
		.expect_confirm(USE_WALLET_PROMPT, true);

	let chain = chain::configure(
		"Select a chain (type to filter)",
		"Which chain would you like to interact with?",
		node_url,
		&None,
		|_| true,
		&mut cli,
	)
	.await?;
	assert_eq!(chain.url, Url::parse(node_url)?);

	let call_chain = call_config.configure_call(&chain, &mut cli)?;
	assert_eq!(call_chain.function.pallet(), "System");
	assert_eq!(call_chain.function.name(), "remark");
	assert_eq!(call_chain.args, vec!["0x11".to_string()]);
	assert_eq!(call_chain.suri, Some("//Alice".to_string()));
	assert!(call_chain.use_wallet);
	assert!(call_chain.sudo);
	assert_eq!(
		call_chain.display(&chain),
		format!(
			"pop call chain --pallet System --function remark --args \"0x11\" --url {node_url}/ --use-wallet --sudo"
		)
	);
	cli.verify()
}

#[tokio::test]
async fn guide_user_to_configure_predefined_action_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let mut call_config = CallChainCommand::default();
	let mut cli = MockCli::new()
		.expect_select(
			"Select a chain (type to filter)".to_string(),
			Some(true),
			true,
			None,
			1,
			None,
		)
		.expect_input("Which chain would you like to interact with?", node_url.into());
	let chain = chain::configure(
		"Select a chain (type to filter)",
		"Which chain would you like to interact with?",
		node_url,
		&None,
		|_| true,
		&mut cli,
	)
	.await?;
	cli.verify()?;

	let mut cli = MockCli::new()
		.expect_select(
			"What would you like to do?",
			Some(true),
			true,
			Some(
				std::iter::once((
					"Other".to_string(),
					"Explore all pallets and functions".to_string(),
				))
				.chain(supported_actions(&chain.pallets).into_iter().map(|action| {
					(action.description().to_string(), action.pallet_name().to_string())
				}))
				.collect::<Vec<_>>(),
			),
			2,
			None,
		)
		.expect_input("Enter the value for the parameter: id", "10000".into())
		.expect_select("Select the value for the parameter: admin", Some(true), true, None, 0, None)
		.expect_input(
			"Enter the value for the parameter: Id",
			"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".into(),
		)
		.expect_input("Enter the value for the parameter: min_balance", "2000".into())
		.expect_input("Signer of the extrinsic:", BOB_SURI.into());

	let call_chain = call_config.configure_call(&chain, &mut cli)?;
	assert_eq!(call_chain.function.pallet(), "Assets");
	assert_eq!(call_chain.function.name(), "create");
	assert_eq!(call_chain.suri, Some("//Bob".to_string()));
	assert!(!call_chain.sudo);
	cli.verify()
}

#[tokio::test]
async fn prepare_extrinsic_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let mut call_config = Call {
		function: CallItem::Function(Function {
			pallet: "WrongName".to_string(),
			name: "WrongName".to_string(),
			..Default::default()
		}),
		args: vec!["0x11".to_string()],
		suri: Some("//Alice".to_string()),
		use_wallet: false,
		skip_confirm: false,
		sudo: false,
	};
	let mut cli = MockCli::new();
	assert!(call_config.prepare_extrinsic(&client, &mut cli).is_err());
	let pallets = parse_chain_metadata(&client)?;
	if let CallItem::Function(ref mut function) = call_config.function {
		function.pallet = "System".to_string();
	}
	assert!(call_config.prepare_extrinsic(&client, &mut cli).is_err());

	cli = MockCli::new().expect_info("Encoded call data: 0x00000411");
	call_config.function = pop_chains::find_callable_by_name(&pallets, "System", "remark")?.clone();
	let xt = call_config.prepare_extrinsic(&client, &mut cli)?;
	assert_eq!(xt.call_name(), "remark");
	assert_eq!(xt.pallet_name(), "System");
	cli.verify()
}

#[tokio::test]
async fn user_cancel_submit_extrinsic_from_call_data_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let node_url = node.ws_url();
	let client = set_up_client(node_url).await?;
	let call_config = CallChainCommand {
		url: Some(Url::parse(node_url)?),
		call_data: Some("0x00000411".to_string()),
		..Default::default()
	};
	let mut cli = MockCli::new()
		.expect_confirm(USE_WALLET_PROMPT, false)
		.expect_input("Signer of the extrinsic:", "//Bob".into())
		.expect_confirm("Do you want to submit the extrinsic?", false)
		.expect_outro_cancel("Extrinsic with call data 0x00000411 was not submitted.");
	call_config
		.submit_extrinsic_from_call_data(&client, &Url::parse(node_url)?, "0x00000411", &mut cli)
		.await?;
	cli.verify()
}

#[tokio::test]
async fn query_storage_from_test_node_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	let system_pallet = pallets.iter().find(|p| p.name == "System").unwrap();
	let number_storage = system_pallet.state.iter().find(|s| s.name == "Number").unwrap();
	let result = number_storage.query(&client, vec![]).await?;
	assert!(result.is_some());
	assert!(matches!(result.unwrap().value, ValueDef::Primitive(_)));
	Ok(())
}

#[tokio::test]
async fn query_constants_from_test_node_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	let system_pallet = pallets.iter().find(|p| p.name == "System").unwrap();
	let version_constant = system_pallet.constants.iter().find(|c| c.name == "Version").unwrap();
	assert!(matches!(version_constant.value.value, ValueDef::Composite(_)));
	Ok(())
}

#[tokio::test]
async fn query_storage_with_composite_key_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let cmd = CallChainCommand {
		pallet: Some("Assets".to_string()),
		function: Some("Account".to_string()),
		args: vec![
			"10000".to_string(),
			"0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d".to_string(),
		],
		url: Some(Url::parse(node.ws_url())?),
		skip_confirm: true,
		..Default::default()
	};
	cmd.execute().await
}

#[tokio::test]
async fn display_metadata_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	let chain = Chain { url: Url::parse(node.ws_url())?, client, pallets: pallets.clone() };

	let cmd = CallChainCommand { metadata: true, ..Default::default() };
	let mut cli = MockCli::new().expect_info(format!("Available pallets ({}):\n", pallets.len()));
	assert!(cmd.display_metadata(&chain, &mut cli).is_ok());
	cli.verify()?;

	let cmd = CallChainCommand {
		pallet: Some("System".to_string()),
		metadata: true,
		..Default::default()
	};
	let mut cli = MockCli::new().expect_info("Pallet: System\n".to_string());
	assert!(cmd.display_metadata(&chain, &mut cli).is_ok());
	cli.verify()
}

#[tokio::test]
async fn show_pallet_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	let metadata = client.metadata();
	let registry = metadata.types();

	let system_pallet = pallets.iter().find(|p| p.name == "System").unwrap();
	let mut cli = MockCli::new().expect_info("Pallet: System\n");
	assert!(show_pallet(system_pallet, registry, &mut cli).is_ok());
	Ok(())
}
