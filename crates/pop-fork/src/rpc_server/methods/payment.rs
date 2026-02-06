// SPDX-License-Identifier: GPL-3.0

//! Payment RPC methods for fee estimation.
//!
//! These methods provide fee estimation for polkadot.js compatibility.

use crate::{
	Blockchain,
	rpc_server::{RpcServerError, parse_block_hash, parse_hex_bytes, types::HexString},
	strings::rpc_server::runtime_api,
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use scale::Encode;
use std::sync::Arc;

/// Payment RPC methods.
#[rpc(server, namespace = "payment")]
pub trait PaymentApi {
	/// Query transaction fee info.
	///
	/// Returns runtime dispatch information including weight, class, and partial fee.
	#[method(name = "queryInfo")]
	async fn query_info(&self, extrinsic: String, at: Option<String>) -> RpcResult<String>;

	/// Query detailed fee information.
	///
	/// Returns detailed fee breakdown including base fee, length fee, and weight fee.
	#[method(name = "queryFeeDetails")]
	async fn query_fee_details(&self, extrinsic: String, at: Option<String>) -> RpcResult<String>;
}

/// Implementation of payment RPC methods.
pub struct PaymentApi {
	blockchain: Arc<Blockchain>,
}

impl PaymentApi {
	/// Create a new PaymentApi instance.
	pub fn new(blockchain: Arc<Blockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl PaymentApiServer for PaymentApi {
	async fn query_info(&self, extrinsic: String, at: Option<String>) -> RpcResult<String> {
		let ext_bytes = parse_hex_bytes(&extrinsic, "extrinsic")?;

		let block_hash = match at {
			Some(hash) => parse_block_hash(&hash)?,
			None => self.blockchain.head_hash().await,
		};

		// Build call params: extrinsic bytes + length as u32
		let mut params = ext_bytes.clone();
		params.extend((ext_bytes.len() as u32).encode());

		match self
			.blockchain
			.call_at_block(block_hash, runtime_api::QUERY_INFO, &params)
			.await
		{
			Ok(Some(result)) => {
				// Return raw hex result - let the client decode it
				Ok(HexString::from_bytes(&result).into())
			},
			Ok(None) =>
				Err(RpcServerError::Internal("Runtime call returned no result".to_string()).into()),
			Err(e) => Err(RpcServerError::Internal(format!("Runtime call failed: {}", e)).into()),
		}
	}

	async fn query_fee_details(&self, extrinsic: String, at: Option<String>) -> RpcResult<String> {
		let ext_bytes = parse_hex_bytes(&extrinsic, "extrinsic")?;

		let block_hash = match at {
			Some(hash) => parse_block_hash(&hash)?,
			None => self.blockchain.head_hash().await,
		};

		// Build call params: extrinsic bytes + length as u32
		let mut params = ext_bytes.clone();
		params.extend((ext_bytes.len() as u32).encode());

		match self
			.blockchain
			.call_at_block(block_hash, runtime_api::QUERY_FEE_DETAILS, &params)
			.await
		{
			Ok(Some(result)) => {
				// Return raw hex result - let the client decode it
				Ok(HexString::from_bytes(&result).into())
			},
			Ok(None) =>
				Err(RpcServerError::Internal("Runtime call returned no result".to_string()).into()),
			Err(e) => Err(RpcServerError::Internal(format!("Runtime call failed: {}", e)).into()),
		}
	}
}
