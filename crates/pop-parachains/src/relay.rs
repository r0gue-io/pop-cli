// SPDX-License-Identifier: GPL-3.0

use crate::{call, DynamicPayload, Error};
use scale::{Decode, Encode};
use sp_core::twox_128;
use subxt::{
	config::BlockHash,
	dynamic::{self, Value},
	events::StaticEvent,
	ext::{scale_decode::DecodeAsType, scale_encode::EncodeAsType, sp_core},
	OnlineClient, PolkadotConfig,
};

/// Clears the DMPQ state for the given parachain IDs.
///
/// # Arguments
/// * `client` - Client for the network which state is to be modified.
/// * `para_ids` - List of ids to build the keys that will be mutated.
pub async fn clear_dmpq(
	client: OnlineClient<PolkadotConfig>,
	para_ids: &[u32],
) -> Result<impl BlockHash, Error> {
	// Wait for blocks to be produced.
	let mut sub = client.blocks().subscribe_finalized().await?;
	for _ in 0..2 {
		sub.next().await;
	}

	// Generate storage keys to be removed
	let clear_dmq_keys = generate_storage_keys(para_ids);

	// Submit calls to remove specified keys
	let kill_storage = construct_kill_storage_call(clear_dmq_keys);
	let sudo = subxt_signer::sr25519::dev::alice();
	let sudo_call = call::construct_sudo_extrinsic(kill_storage);
	Ok(client.tx().sign_and_submit_default(&sudo_call, &sudo).await?)
}

fn construct_kill_storage_call(keys: Vec<Vec<u8>>) -> DynamicPayload {
	dynamic::tx(
		"System",
		"kill_storage",
		vec![Value::unnamed_composite(keys.into_iter().map(Value::from_bytes))],
	)
}

fn generate_storage_keys(para_ids: &[u32]) -> Vec<Vec<u8>> {
	let dmp = twox_128("Dmp".as_bytes());
	let dmp_queue_heads = twox_128("DownwardMessageQueueHeads".as_bytes());
	let dmp_queues = twox_128("DownwardMessageQueues".as_bytes());
	let mut clear_dmq_keys = Vec::<Vec<u8>>::new();
	for id in para_ids {
		let id = id.to_le_bytes();
		// DMP Queue Head
		let mut key = dmp.to_vec();
		key.extend(&dmp_queue_heads);
		key.extend(sp_core::twox_64(&id));
		key.extend(id);
		clear_dmq_keys.push(key);
		// DMP Queue
		let mut key = dmp.to_vec();
		key.extend(&dmp_queues);
		key.extend(sp_core::twox_64(&id));
		key.extend(id);
		clear_dmq_keys.push(key);
	}
	clear_dmq_keys
}

/// A supported relay chain.
#[derive(Debug, PartialEq)]
pub enum RelayChain {
	/// Paseo.
	PaseoLocal,
	/// Westend.
	WestendLocal,
}

impl RelayChain {
	/// Attempts to convert a chain identifier into a supported `RelayChain` variant.
	///
	/// # Arguments
	/// * `id` - The relay chain identifier.
	pub fn from(id: &str) -> Option<RelayChain> {
		match id {
			"paseo-local" => Some(RelayChain::PaseoLocal),
			"westend-local" => Some(RelayChain::WestendLocal),
			_ => None,
		}
	}
}

/// A event emitted when an id has been registered.
#[derive(Debug, Encode, Decode, DecodeAsType, EncodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Reserved {
	/// The id that has been reserved.
	pub para_id: u32,
}
impl StaticEvent for Reserved {
	const PALLET: &'static str = "Registrar";
	const EVENT: &'static str = "Reserved";
}

#[cfg(test)]
mod tests {
	use super::*;
	use subxt::ext::sp_core::twox_64;
	use RelayChain::*;

	#[test]
	fn construct_kill_storage_call_works() {
		let keys = vec!["key".as_bytes().to_vec()];
		assert_eq!(
			construct_kill_storage_call(keys.clone()),
			dynamic::tx(
				"System",
				"kill_storage",
				vec![Value::unnamed_composite(keys.into_iter().map(Value::from_bytes))],
			)
		)
	}

	#[test]
	fn generate_storage_keys_works() {
		let para_ids = vec![1_000, 4_385];
		let dmp = twox_128("Dmp".as_bytes());
		let dmp_queue_heads = [dmp, twox_128("DownwardMessageQueueHeads".as_bytes())].concat();
		let dmp_queues = [dmp, twox_128("DownwardMessageQueues".as_bytes())].concat();

		assert_eq!(
			generate_storage_keys(&para_ids),
			para_ids
				.iter()
				.flat_map(|id| {
					let id = id.to_le_bytes().to_vec();
					[
						// DMP Queue Head
						[dmp_queue_heads.clone(), twox_64(&id).to_vec(), id.clone()].concat(),
						// DMP Queue
						[dmp_queues.clone(), twox_64(&id).to_vec(), id].concat(),
					]
				})
				.collect::<Vec<_>>()
		)
	}

	#[test]
	fn supported_relay_chains() {
		for (s, e) in [
			// Only chains with sudo supported
			("paseo-local", Some(PaseoLocal)),
			("westend-local", Some(WestendLocal)),
			("kusama-local", None),
			("polkadot-local", None),
		] {
			assert_eq!(RelayChain::from(s), e)
		}
	}
}
