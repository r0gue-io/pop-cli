use std::path::PathBuf;
use sp_core::Bytes;
use clap::Args;
use cliclack::{intro, log};
use anyhow::anyhow;

use crate::{
    style::style, signer::{parse_hex_bytes, create_signer}, 
    engines::contract_engine::{instantiate_smart_contract,dry_run_gas_estimate_instantiate}
};

use sp_weights::Weight;
use contract_extrinsics::{BalanceVariant, ExtrinsicOptsBuilder, InstantiateExec, InstantiateCommandBuilder, TokenMetadata};
use contract_build::ManifestPath;
use subxt::PolkadotConfig as DefaultConfig;
use subxt_signer::sr25519::Keypair;
use ink_env::{DefaultEnvironment, Environment};


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
    /// Transfers an initial balance to the instantiated contract
    #[clap(name = "value", long, default_value = "0")]
    value: BalanceVariant<<DefaultEnvironment as Environment>::Balance>,
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
        let instantiate_exec = 
            self.set_up_deployment().await?;
        
        let weight_limit;
        if self.gas_limit.is_some() && self.proof_size.is_some() {
            weight_limit = Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap());
        }
        else {
            log::info("Doing a dry run to estimate the gas...")?;
            weight_limit = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
            log::info(format!(
                "Gas limit {:?}",
                weight_limit
            ))?;
        }
        log::info("Uploading and instantiating the contract...")?;
        instantiate_smart_contract(instantiate_exec, weight_limit).await.map_err(|err| anyhow!(
            "{} {}",
            "ERROR:",
            format!("{err:?}")
        ))?;
        Ok(())
    }

    async fn set_up_deployment(&self) -> anyhow::Result<InstantiateExec<
        DefaultConfig,
        DefaultEnvironment,
        Keypair,
        >> {
            // If the user specify a path (not current directory) have to manually add Cargo.toml here or ask to the user the specific path
            let manifest_path ;
            if self.path.is_some(){
                let full_path: PathBuf = PathBuf::from(self.path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml");
                manifest_path = ManifestPath::try_from(Some(full_path))?;
            }
            else {
                manifest_path = ManifestPath::try_from(self.path.as_ref())?;
            }
    
            let token_metadata =
            TokenMetadata::query::<DefaultConfig>(&self.url).await?;
    
            let signer = create_signer(&self.suri)?;
            let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
                .manifest_path(Some(manifest_path))
                .url(self.url.clone())
                .done();
    
            let instantiate_exec: InstantiateExec<
                DefaultConfig,
                DefaultEnvironment,
                Keypair,
            > = InstantiateCommandBuilder::new(extrinsic_opts)
                .constructor(self.constructor.clone())
                .args(self.args.clone())
                .value(self.value.denominate_balance(&token_metadata)?)
                .gas_limit(self.gas_limit)
                .proof_size(self.proof_size)
                .salt(self.salt.clone())
                .done()
                .await?;
            return Ok(instantiate_exec);
    }
}
