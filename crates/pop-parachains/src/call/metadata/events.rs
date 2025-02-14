// SPDX-License-Identifier: GPL-3.0

use scale::{Decode, Encode};
use subxt::{
	blocks::ExtrinsicEvents,
	events::StaticEvent,
	ext::{scale_decode::DecodeAsType, scale_encode::EncodeAsType},
	SubstrateConfig,
};

use crate::Error;

#[derive(Debug, Encode, Decode, DecodeAsType, EncodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Reserved {
	pub para_id: u32,
}
impl StaticEvent for Reserved {
	const PALLET: &'static str = "Registrar";
	const EVENT: &'static str = "Reserved";
}

/// Extracts the `para_id` field from a `Reserved` event.
///
/// # Arguments
/// * `events` - The extrinsic events from a transaction.
pub async fn extract_para_id_from_event(
	events: &ExtrinsicEvents<SubstrateConfig>,
) -> Result<u32, Error> {
	let reserved_event = events.find_first::<Reserved>()?;
	reserved_event
		.map(|event| event.para_id)
		.ok_or(Error::EventNotFound("Reserved".to_string()))
}
