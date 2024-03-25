use crate::Result;
use anyhow::anyhow;
use clap::{Args, Subcommand};
use console::style;
use log::debug;
use serde::Deserialize;
use smoldot_light::{
    platform::DefaultPlatform, AddChainConfig, AddChainConfigJsonRpc, AddChainSuccess, ChainId,
    Client, JsonRpcResponses,
};
use std::{iter, num::NonZeroU32, pin::Pin, sync::Arc};
use thousands::Separable;
use tokio_stream::{Stream, StreamExt, StreamMap};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct FollowArgs {
    #[command(subcommand)]
    pub(crate) command: FollowCommands,
}

#[derive(Subcommand)]
pub(crate) enum FollowCommands {
    /// Follow one or more chains.
    #[clap(alias = "c")]
    Chain(FollowChainCommand),
}

#[derive(Args)]
pub(crate) struct FollowChainCommand {
    /// The path to a relay chain's chain-spec.
    #[arg(short, long)]
    relay_chain: String,
    /// The path to a parachain's chain-spec.
    #[arg(short, long)]
    parachain: Option<Vec<String>>,
}

impl FollowChainCommand {
    pub(crate) async fn execute(&self) -> Result<()> {
        //tracing_subscriber::fmt().init();

        let mut client = Client::new(DefaultPlatform::new(
            env!("CARGO_PKG_NAME").into(),
            env!("CARGO_PKG_VERSION").into(),
        ));

        let json_rpc_config = AddChainConfigJsonRpc::Enabled {
            max_pending_requests: NonZeroU32::new(128).expect("value is valid"),
            max_subscriptions: 1024,
        };

        let mut chains = StreamMap::new();

        // Add relay chain
        let specification = std::fs::read_to_string(&self.relay_chain)?;
        let AddChainSuccess {
            chain_id: relay_chain_id,
            json_rpc_responses,
        } = client
            .add_chain(AddChainConfig {
                specification: &specification,
                json_rpc: json_rpc_config.clone(),
                potential_relay_chains: iter::empty(),
                // todo: save/read database using cache
                database_content: "",
                user_data: (),
            })
            .map_err(|e| anyhow!("{e}"))?;
        chains.insert(
            Chain::new(relay_chain_id, Self::name(&specification)?),
            Self::stream(json_rpc_responses),
        );

        // Add parachain(s)
        if let Some(parachains) = &self.parachain {
            for parachain in parachains {
                let specification = std::fs::read_to_string(&parachain)?;
                let AddChainSuccess {
                    chain_id,
                    json_rpc_responses,
                } = client
                    .add_chain(AddChainConfig {
                        specification: &specification,
                        json_rpc: json_rpc_config.clone(),
                        // todo: save/read database using cache
                        database_content: "",
                        user_data: (),
                        potential_relay_chains: [relay_chain_id].into_iter(),
                    })
                    .map_err(|e| anyhow!("{e}"))?;
                chains.insert(
                    Chain::new(chain_id, Self::name(&specification)?),
                    Self::stream(json_rpc_responses),
                );
            }
        }

        // Subscribe to best block on each chain
        for chain in chains.keys() {
            client
                .json_rpc_request(
                    r#"{"id":1,"jsonrpc":"2.0","method":"chain_subscribeNewHeads","params":[]}"#,
                    chain.id,
                )
                .unwrap();
        }

        loop {
            let (chain, response) = chains.next().await.unwrap();
            match serde_json::from_str(&response) {
                Ok(response) => match response {
                    Response::NewHead { header } => {
                        let block_number = u32::from_str_radix(&header.number[2..], 16).unwrap();
                        println!(
                            "{}: #{}{}",
                            chain.name,
                            block_number.separate_with_commas(),
                            style(format!(
                                ", parent hash: {}, state root: {}, extrinsics root: {}",
                                header.parent_hash, header.state_root, header.extrinsics_root
                            ))
                            .dim()
                        );
                    }
                },
                Err(e) => {
                    debug!("unable to deserialize response: {e} {response}");
                }
            }
        }
    }

    // Convert JSON RPC responses to a stream
    fn stream(
        json_rpc_responses: Option<JsonRpcResponses<Arc<DefaultPlatform>>>,
    ) -> Pin<Box<dyn Stream<Item = String> + Send>> {
        let mut json_rpc_responses = json_rpc_responses.unwrap();
        let stream = Box::pin(async_stream::stream! {
              while let Some(item) = json_rpc_responses.next().await {
                  yield item;
              }
        }) as Pin<Box<dyn Stream<Item = String> + Send>>;
        stream
    }

    // Extract the name from a chain spec.
    fn name(specification: &str) -> Result<String> {
        smoldot::chain_spec::ChainSpec::from_json_bytes(specification)
            .map(|cs| cs.name().into())
            .map_err(|e| anyhow!("error parsing chain-spec: {e}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Chain {
    id: ChainId,
    name: String,
}
impl Chain {
    fn new(id: ChainId, name: String) -> Self {
        Self { id, name }
    }
}

#[derive(Deserialize)]
#[serde(tag = "method", content = "params")]
enum Response {
    #[serde(alias = "chain_newHead")]
    NewHead {
        #[serde(alias = "result")]
        header: Header,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    parent_hash: String,
    number: String,
    state_root: String,
    extrinsics_root: String,
}
