use std::path::PathBuf;
use sp_core::Bytes;

use clap::Args;
use cliclack::intro;

use crate::{style::style, helpers::parse_hex_bytes};

// use crate::engines::contract_engine::create_smart_contract;

#[derive(Args)]
pub struct UpContractCommand {
    /// Path to a contract build folder
    #[arg(short = 'p', long = "path")]
    path: Option<PathBuf>,
    /// The name of the contract constructor to call
    #[clap(name = "constructor", long, default_value = "new")]
    constructor: String,
    /// The constructor arguments, encoded as strings
    #[clap(long, num_args = 0..)]
    args: Vec<String>,
    /// Maximum amount of gas to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the
    /// instantiation.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// Maximum proof size for this instantiation.
    /// If not specified will perform a dry-run to estimate the proof size required.
    #[clap(long)]
    proof_size: Option<u64>,
    /// A salt used in the address derivation of the new contract. Use to create multiple
    /// instances of the same contract code from the same account.
    #[clap(long, value_parser = parse_hex_bytes)]
    salt: Option<Bytes>,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Secret key URI for the account deploying the contract.
    ///
    /// e.g.
    /// - for a dev account "//Alice"
    /// - with a password "//Alice///SECRET_PASSWORD"
    #[clap(name = "suri", long, short)]
    suri: String,
}

impl UpContractCommand {
    pub(crate) async fn execute(&self) -> anyhow::Result<()> {
        intro(format!(
            "{}: Deploy a smart contract",
            style(" Pop CLI ").black().on_magenta()
        ))?;
        if self.gas_limit.is_some() && self.proof_size.is_some() {
            //initiate
        }
        else {
            //dry run
            //initiate
        }
        //create_smart_contract(self.name.clone(), &self.path)?;
        Ok(())
    }
}
