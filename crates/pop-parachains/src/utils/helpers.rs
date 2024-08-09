// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use std::{
	fs::{self, OpenOptions},
	io::{self, stdin, stdout, Write},
	path::Path,
};
use subxt::{ext::sp_core, OnlineClient, PolkadotConfig};

pub(crate) fn sanitize(target: &Path) -> Result<(), Error> {
	if target.exists() {
		print!("\"{}\" folder exists. Do you want to clean it? [y/n]: ", target.display());
		stdout().flush()?;

		let mut input = String::new();
		stdin().read_line(&mut input)?;

		if input.trim().to_lowercase() == "y" {
			fs::remove_dir_all(target).map_err(|_| Error::Aborted)?;
		} else {
			return Err(Error::Aborted);
		}
	}
	Ok(())
}

/// Clears the DMPQ state for the given chains.
/// Assumes pallet-sudo is present in the runtime.
///
/// # Arguments
///
/// * `client` - Client for the network which state is to be modified.
/// * `para_ids` - List of ids to build the keys that will be mutated.
pub async fn clear_dmpq(
	client: OnlineClient<PolkadotConfig>,
	para_ids: &[u32],
) -> Result<(), Box<dyn std::error::Error>> {
	use subxt_signer::sr25519::dev;

	#[subxt::subxt(runtime_metadata_path = "./src/utils/artifacts/paseo-local.scale")]
	mod paseo_local {}
	type RuntimeCall = paseo_local::runtime_types::paseo_runtime::RuntimeCall;

	let sudo = dev::alice();

	// Wait for blocks to be produced.
	let mut sub = client.blocks().subscribe_finalized().await.unwrap();
	for _ in 0..2 {
		sub.next().await;
	}

	let dmp = sp_core::twox_128("Dmp".as_bytes());
	let dmp_queues = sp_core::twox_128("DownwardMessageQueues".as_bytes());
	let dmp_queue_heads = sp_core::twox_128("DownwardMessageQueueHeads".as_bytes());

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

	// Craft calls to dispatch
	let kill_storage =
		RuntimeCall::System(paseo_local::system::Call::kill_storage { keys: clear_dmq_keys });
	let sudo_call = paseo_local::tx().sudo().sudo(kill_storage);

	// Dispatch and watch tx
	let _sudo_call_events =
		client.tx().sign_and_submit_then_watch_default(&sudo_call, &sudo).await?;

	Ok(())
}

/// Check if the initial endowment input by the user is a valid balance.
///
/// # Arguments
///
/// * `initial_endowment` - initial endowment amount to be checked for validity.
pub fn is_initial_endowment_valid(initial_endowment: &str) -> bool {
	initial_endowment.parse::<u128>().is_ok()
		|| is_valid_bitwise_left_shift(initial_endowment).is_ok()
}
// Auxiliar method to check if the endowment input with a shift left (1u64 << 60) format is valid.
// Parse the self << rhs format and check the shift left operation is valid.
fn is_valid_bitwise_left_shift(initial_endowment: &str) -> Result<u128, Error> {
	let v: Vec<&str> = initial_endowment.split(" << ").collect();
	if v.len() < 2 {
		return Err(Error::EndowmentError);
	}
	let left = v[0]
		.split("u") // parse 1u64 characters
		.take(1)
		.collect::<String>()
		.parse::<u128>()
		.or_else(|_e| Err(Error::EndowmentError))?;
	let right = v[1]
		.chars()
		.filter(|c| c.is_numeric()) // parse 1u64 characters
		.collect::<String>()
		.parse::<u32>()
		.or_else(|_e| Err(Error::EndowmentError))?;
	left.checked_shl(right).ok_or(Error::EndowmentError)
}

pub(crate) fn write_to_file(path: &Path, contents: &str) -> Result<(), Error> {
	let mut file = OpenOptions::new()
		.write(true)
		.truncate(true)
		.create(true)
		.open(path)
		.map_err(|err| Error::RustfmtError(err))?;

	file.write_all(contents.as_bytes()).map_err(|err| Error::RustfmtError(err))?;

	if path.extension().map_or(false, |ext| ext == "rs") {
		let output = std::process::Command::new("rustfmt")
			.arg(path.to_str().unwrap())
			.output()
			.map_err(|err| Error::RustfmtError(err))?;

		if !output.status.success() {
			return Err(Error::RustfmtError(io::Error::new(
				io::ErrorKind::Other,
				"rustfmt exited with non-zero status code",
			)));
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::generator::parachain::ChainSpec;
	use askama::Template;
	use std::env::var;
	use tempfile::tempdir;

	#[test]
	fn test_write_to_file() -> Result<(), Box<dyn std::error::Error>> {
		let temp_dir = tempdir()?;
		let chainspec = ChainSpec {
			token_symbol: "DOT".to_string(),
			decimals: 6,
			initial_endowment: "1000000".to_string(),
		};
		let file_path = temp_dir.path().join("file.rs");
		let _ = fs::write(&file_path, "");
		write_to_file(&file_path, chainspec.render().expect("infallible").as_ref())?;
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("file.rs")).expect("Failed to read file");

		assert!(generated_file_content
			.contains("properties.insert(\"tokenSymbol\".into(), \"DOT\".into());"));
		assert!(generated_file_content
			.contains("properties.insert(\"tokenDecimals\".into(), 6.into());"));
		assert!(generated_file_content.contains("1000000"));

		Ok(())
	}

	#[test]
	fn test_is_initial_endowment_valid() {
		assert_eq!(is_initial_endowment_valid("100000"), true);
		assert_eq!(is_initial_endowment_valid("1u64 << 60"), true);
		assert_eq!(is_initial_endowment_valid("wrong"), false);
		assert_eq!(is_initial_endowment_valid(" "), false);
	}

	#[test]
	fn test_left_shift() {
		// Values from https://stackoverflow.com/questions/56392875/how-can-i-initialize-a-users-balance-in-a-substrate-blockchain
		assert_eq!(is_valid_bitwise_left_shift("1 << 60").unwrap(), 1152921504606846976);
		let result = is_valid_bitwise_left_shift("wrong");
		assert!(result.is_err());
	}
}
