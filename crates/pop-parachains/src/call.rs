// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::metadata::process_extrinsic_args};
use pop_common::create_signer;
use subxt::{
	dynamic::Value,
	tx::{DynamicPayload, Payload},
	OnlineClient, SubstrateConfig,
};

pub async fn set_up_api(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	Ok(api)
}

pub async fn construct_extrinsic(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
	extrinsic_name: &str,
	args: Vec<String>,
) -> Result<DynamicPayload, Error> {
	let parsed_args: Vec<Value> =
		process_extrinsic_args(api, pallet_name, extrinsic_name, args).await?;
	Ok(subxt::dynamic::tx(pallet_name, extrinsic_name, parsed_args))
}

pub async fn sign_and_submit_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	tx: DynamicPayload,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let result = api
		.tx()
		.sign_and_submit_then_watch_default(&tx, &signer)
		.await?
		.wait_for_finalized()
		.await?
		.wait_for_success()
		.await?;
	Ok(format!("{:?}", result.extrinsic_hash()))
}

pub fn encode_call_data(
	api: &OnlineClient<SubstrateConfig>,
	tx: &DynamicPayload,
) -> Result<String, Error> {
	let call_data = tx.encode_call_data(&api.metadata())?;
	Ok(format!("0x{}", hex::encode(call_data)))
}
