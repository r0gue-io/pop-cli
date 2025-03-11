// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use clap::{Args, Subcommand};
use ismp::{
	consensus::{StateMachineHeight, StateMachineId},
	host::StateMachine,
	messaging::{Message, Proof, ResponseMessage},
	router::{Request, RequestResponse},
};
use sp_core::{bytes::from_hex, H256};
use std::collections::HashMap;
use substrate_state_machine::{HashAlgorithm, StateMachineProof, SubstrateStateProof};
use subxt::{
	backend::{legacy::rpc_methods::ReadProof, rpc::RpcClient},
	dynamic::Value,
	ext::{
		codec::{Decode, Encode},
		scale_value::scale::{decode_as_type, DecodeError, PortableRegistry},
		sp_runtime::scale_info::{MetaType, Registry, TypeInfo},
	},
	rpc_params,
	utils::to_hex,
	OnlineClient, PolkadotConfig,
};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct RelayArgs {
	#[command(subcommand)]
	pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
	/// Query requests for a set of commitments.
	#[clap(alias = "q")]
	Query(QueryCommandArgs),
	#[clap(alias = "g")]
	Get(GetCommandArgs),
}

#[derive(Args)]
pub struct QueryCommandArgs {
	///
	#[arg(short = 'c', long, value_delimiter = ',', required = true)]
	pub(crate) commitment: Vec<H256>,
	///
	#[arg(long)]
	pub(crate) rpc: String,
}

#[derive(Args)]
pub struct GetCommandArgs {
	#[arg(short = 'c', long, value_delimiter = ',', required = true)]
	pub(crate) commitment: Vec<H256>,
	///
	#[arg(short = 's', long)]
	pub(crate) source: String,
	///
	#[arg(short = 'd', long, value_delimiter = ',', required = true)]
	pub(crate) dest: Vec<String>,
}

impl Command {
	pub(crate) async fn execute(self) -> Result<()> {
		match self {
			Command::Query(args) => {
				let commitments = commitments(args.commitment.clone());
				let rpc = RpcClient::from_insecure_url(args.rpc).await?;
				let requests = query_requests(&rpc, commitments).await?;
				println!("{requests:?}");
			},
			Command::Get(args) => {
				// Resolve destination RPCs to para ids.
				let mut destinations = HashMap::new();
				for dest in args.dest {
					let rpc = RpcClient::from_insecure_url(dest).await?;
					let client =
						OnlineClient::<PolkadotConfig>::from_rpc_client(rpc.clone()).await?;
					let para_id = self::para_id(&client).await?;
					destinations.insert(para_id, (client, rpc));
				}

				// Resolve requests from provided commitments.
				let rpc = RpcClient::from_insecure_url(args.source).await?;
				let client = OnlineClient::<PolkadotConfig>::from_rpc_client(rpc.clone()).await?;
				let commitments = commitments(args.commitment.clone());
				let requests = query_requests(&rpc, commitments).await?;

				let mut messages = Vec::new();
				for request in requests {
					println!("Processing {request:?}");

					// Determine request and destination.
					let get = request.get_request().expect("only get requests supported currently");
					let StateMachine::Polkadot(dest) = request.dest_chain() else {
						panic!("only polkadot parachains are supported")
					};

					// Get state proof from destination RPC.
					let rpc = &destinations[&dest].1;
					let block = block_hash(&rpc, get.height)
						.await?
						.expect(&format!("block hash not found for #{}", get.height));
					let proof = state_read_proof(&rpc, &get.keys, &block).await?;
					println!("{proof:?}");

					let proof = SubstrateStateProof::StateProof(StateMachineProof {
						hasher: HashAlgorithm::Blake2,
						storage_proof: proof.proof.into_iter().map(|b| b.0).collect(),
					});

					// Prepare response.
					let response = Message::Response(ResponseMessage {
						proof: Proof {
							height: StateMachineHeight {
								id: StateMachineId {
									state_id: get.dest,
									consensus_state_id: *b"PARA",
								},
								height: get.height,
							},
							proof: proof.encode(),
						},
						datagram: RequestResponse::Request(vec![Request::Get(get)]),
						signer: vec![],
					});
					println!("Response: {response:?}");
					messages.push(response);
				}

				// Submit results back to source.
				let tx = subxt::dynamic::tx("Ismp", "handle_unsigned", vec![encode(messages)?]);
				let tx = client.tx().create_unsigned(&tx)?;
				let encoded = to_hex(tx.encoded());
				println!("Encoded call: {encoded}");
				let events = tx.submit_and_watch().await?.wait_for_finalized_success().await?;
				println!("Submitted: {events:?}");
			},
		}
		Ok(())
	}
}

// Based on https://github.com/paritytech/polkadot-staking-miner/blob/4406fffc7130a57d80591cc7f55c0867818f3e60/src/epm.rs#L506-L529
fn encode<T: TypeInfo + 'static + Encode>(val: T) -> Result<Value, DecodeError> {
	fn make_type<T: TypeInfo + 'static>() -> (u32, PortableRegistry) {
		let m = MetaType::new::<T>();
		let mut types = Registry::new();
		let id = types.register_type(&m);
		let portable_registry: PortableRegistry = types.into();

		(id.id, portable_registry)
	}

	let (ty_id, types) = make_type::<T>();
	let bytes = val.encode();
	decode_as_type(&mut bytes.as_ref(), ty_id, &types).map(|v| v.remove_context())
}

async fn state_read_proof(
	rpc: &RpcClient,
	keys: &Vec<Vec<u8>>,
	at: &H256,
) -> Result<ReadProof<H256>> {
	let keys: Vec<_> = keys.iter().map(|k| to_hex(k)).collect();
	let params = rpc_params![keys, at].build();
	let response = rpc.request_raw("state_getReadProof", params).await?;
	Ok(serde_json::from_str(response.get())?)
}

async fn block_hash(rpc: &RpcClient, block_number: u64) -> Result<Option<H256>> {
	let params = rpc_params![block_number];
	Ok(rpc.request("chain_getBlockHash", params).await?)
}

async fn query_requests(rpc: &RpcClient, commitments: Vec<LeafIndexQuery>) -> Result<Vec<Request>> {
	let params = rpc_params![commitments].build();
	let response = rpc.request_raw("ismp_queryRequests", params).await?;
	Ok(serde_json::from_str(response.get())?)
}

fn commitments(commitments: Vec<H256>) -> Vec<LeafIndexQuery> {
	commitments
		.into_iter()
		.map(|commitment| LeafIndexQuery { commitment })
		.collect()
}

async fn para_id(client: &OnlineClient<PolkadotConfig>) -> Result<u32> {
	// Query para_id from `parachainInfo parachainId` storage item
	let value = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(from_hex("0x0d715f2646c8f85767b5d2764bb2782604a74d81251e398fd8a0a4d55023bb3f")?)
		.await?
		.unwrap();
	Ok(u32::decode(&mut &value[..])?)
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LeafIndexQuery {
	/// Request or response commitment
	pub commitment: H256,
}
