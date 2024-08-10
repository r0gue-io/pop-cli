// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use sp_core::twox_128;
use subxt::{config::BlockHash, ext::sp_core, OnlineClient, PolkadotConfig};

/// Clears the DMPQ state for the given parachain IDs.
///
/// # Arguments
/// * `relay_chain` - The relay chain.
/// * `client` - Client for the network which state is to be modified.
/// * `para_ids` - List of ids to build the keys that will be mutated.
pub async fn clear_dmpq(
	relay_chain: RelayChain,
	client: OnlineClient<PolkadotConfig>,
	para_ids: &[u32],
) -> Result<impl BlockHash, Error> {
	// Wait for blocks to be produced.
	let mut sub = client.blocks().subscribe_finalized().await?;
	for _ in 0..2 {
		sub.next().await;
	}

	// Generate storage keys to be removed
	let dmp = twox_128("Dmp".as_bytes());
	let dmp_queues = twox_128("DownwardMessageQueues".as_bytes());
	let dmp_queue_heads = twox_128("DownwardMessageQueueHeads".as_bytes());
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

	// Submit calls to remove specified keys
	let sudo = subxt_signer::sr25519::dev::alice();
	match relay_chain {
		RelayChain::PaseoLocal => {
			use paseo_local::{
				runtime_types::paseo_runtime::RuntimeCall::System, system::Call, tx,
			};
			let sudo_call = tx().sudo().sudo(System(Call::kill_storage { keys: clear_dmq_keys }));
			Ok(client.tx().sign_and_submit_default(&sudo_call, &sudo).await?)
		},
		RelayChain::RococoLocal => {
			use rococo_local::{
				runtime_types::rococo_runtime::RuntimeCall::System, system::Call, tx,
			};
			let sudo_call = tx().sudo().sudo(System(Call::kill_storage { keys: clear_dmq_keys }));
			Ok(client.tx().sign_and_submit_default(&sudo_call, &sudo).await?)
		},
	}
}

/// A supported relay chain.
pub enum RelayChain {
	/// Paseo.
	PaseoLocal,
	/// Rococo.
	RococoLocal,
}

impl RelayChain {
	/// Attempts to convert a chain identifier into a supported `RelayChain` variant.
	///
	/// # Arguments
	/// * `id` - The relay chain identifier.
	pub fn from(id: &str) -> Option<RelayChain> {
		match id {
			"paseo-local" => Some(RelayChain::PaseoLocal),
			"rococo-local" => Some(RelayChain::RococoLocal),
			_ => None,
		}
	}
}

// subxt metadata --url ws://127.0.0.1:58774 --pallets System,Sudo > paseo-local.scale
#[subxt::subxt(runtime_metadata_path = "./src/utils/artifacts/paseo-local.scale")]
mod paseo_local {}

// subxt metadata --url ws://127.0.0.1:58774 --pallets System,Sudo > rococo-local.scale
#[subxt::subxt(runtime_metadata_path = "./src/utils/artifacts/rococo-local.scale")]
mod rococo_local {}
