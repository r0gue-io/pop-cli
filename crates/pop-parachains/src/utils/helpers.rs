// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use globalenv::set_var;
use std::{
	fs::{self, OpenOptions},
	io::{self, stdin, stdout, Write},
	path::Path,
};
use zombienet_sdk::{LocalFileSystem, Network};

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

/// Export an environment variable with a node's ws endpoint as its value.
///
/// # Arguments
///
/// * `network` - Name of the network the node is part of.
/// * `name` - Identifier of the node.
/// * `uri` - Endpoint to expose as the variable value.
pub fn export_node_endpoint(network: &str, name: &str, uri: &str) -> Result<String, Error> {
	// Sanity checks for key and value.
	// https://doc.rust-lang.org/std/env/fn.set_var.html#panics
	// Note that this function uses `globalenv` instead of `std::env`
	// though it keep the same safety assumptions.
	if network.contains('=') || network.contains('\0') || uri.contains('\0') {
		return Err(Error::EnvVarSetError);
	}
	let key =
		format!("{}_{}_ENDPOINT", network.to_uppercase(), name.to_uppercase()).replace("-", "_");

	match set_var(&key, uri) {
		Ok(_) => Ok(key),
		Err(_) => Err(Error::EnvVarSetError),
	}
}

pub async fn clear_dmpq(
	network: Network<LocalFileSystem>,
) -> Result<(), Box<dyn std::error::Error>> {
	use subxt::{OnlineClient, PolkadotConfig};
	use subxt_signer::sr25519::dev;

	let relay_endpoint = network.relaychain().nodes()[0].ws_uri();
	//let para_ids: Vec<_> = network.parachains().iter().map(|p| p.para_id()).collect();

	#[subxt::subxt(runtime_metadata_path = "./src/utils/artifacts/paseo-local.scale")]
	mod paseo_local {}
	type RuntimeCall = paseo_local::runtime_types::paseo_runtime::RuntimeCall;

	let api = OnlineClient::<PolkadotConfig>::from_url(relay_endpoint).await?;
	let sudo = dev::alice();

	// Wait for blocks to be produced.
	let mut sub = api.blocks().subscribe_finalized().await.unwrap();
	for _ in 0..2 {
		sub.next().await;
	}

	let mut clear_dmq_keys = Vec::<Vec<u8>>::new();
	clear_dmq_keys.push("0x63f78c98723ddc9073523ef3beefda0c4d7fefc408aac59dbfe80a72ac8e3ce5b6ff6f7d467b87a9e8030000".as_bytes().to_vec());
	clear_dmq_keys.push("0x63f78c98723ddc9073523ef3beefda0c4d7fefc408aac59dbfe80a72ac8e3ce563f5a4efb16ffa83d0070000".as_bytes().to_vec());
	clear_dmq_keys.push("0x63f78c98723ddc9073523ef3beefda0ca95dac46c07a40d91506e7637ec4ba57b6ff6f7d467b87a9e8030000".as_bytes().to_vec());
	clear_dmq_keys.push("0x63f78c98723ddc9073523ef3beefda0ca95dac46c07a40d91506e7637ec4ba5763f5a4efb16ffa83d0070000".as_bytes().to_vec());

	// Craft calls to dispatch
	let kill_storage =
		RuntimeCall::System(paseo_local::system::Call::kill_storage { keys: clear_dmq_keys });
	let sudo_call = paseo_local::tx().sudo().sudo(kill_storage);

	// Dispatch and watch tx
	let _sudo_call_events = api
		.tx()
		.sign_and_submit_then_watch_default(&sudo_call, &sudo)
		.await?
		.wait_for_finalized_success()
		.await?;

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

	#[test]
	fn export_node_endpoint_works() {
		let network = "popchain";
		let node_name = "my-node";
		let node_uri = "ws://127.0.0.1:9944";
		let result = export_node_endpoint(&network, &node_name, &node_uri);
		assert!(result.is_ok());
		assert_eq!(var("POPCHAIN_MY_NODE_ENDPOINT").unwrap(), "ws://127.0.0.1:9944");
	}

	#[test]
	fn export_node_endpoint_errs_if_values_are_not_safe() {
		// Safe values based on:
		// https://doc.rust-lang.org/std/env/fn.set_var.html#panics
		let network = "popchain=";
		let node_name = "n\0de";
		let node_uri = "ws://127.0.0.1:9944\0";
		let result = export_node_endpoint(&network, &node_name, &node_uri);
		assert!(result.is_err());
	}
}
