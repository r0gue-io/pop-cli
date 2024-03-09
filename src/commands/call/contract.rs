use std::path::PathBuf;
use anyhow::anyhow;
use clap::Args;
use cliclack::{clear_screen, intro, outro, set_theme, log, outro_cancel};
use console::style;

use contract_extrinsics::{
	BalanceVariant, ExtrinsicOptsBuilder, CallCommandBuilder, CallExec, TokenMetadata,
};
use contract_build::ManifestPath;
use ink_env::{DefaultEnvironment, Environment};
use sp_weights::Weight;
use subxt::{Config, PolkadotConfig as DefaultConfig};
use subxt_signer::sr25519::Keypair;

use crate::{
	engines::contract_engine::{dry_run_gas_estimate_call, call_smart_contract, dry_run_call},
	signer::create_signer, style::Theme,
};

#[derive(Args)]
pub struct CallContractCommand {
	/// Path to a contract build folder
	#[arg(short = 'p', long)]
	path: Option<PathBuf>,
	/// The address of the the contract to call.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    /// The name of the contract message to call.
    #[clap(long, short)]
    message: String,
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
	/// Websocket endpoint of a node.
	#[clap(name = "url", long, value_parser, default_value = "ws://localhost:9944")]
	url: url::Url,
	/// Secret key URI for the account deploying the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short)]
	suri: String,
    /// Submit the extrinsic for on-chain execution.
    #[clap(short('x'), long)]
    execute: bool,
}

impl CallContractCommand {
	pub(crate) async fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Calling a contract", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

        let token_metadata = TokenMetadata::query::<DefaultConfig>(&self.url).await?;
        let call_exec = self.set_up_call(token_metadata.clone()).await?;

        if !self.execute {
            let mut spinner = cliclack::spinner();
            spinner.start("Calling the contract...");
            let call_dry_run_result = dry_run_call(&call_exec).await?;
            log::info(format!("Result: {}", call_dry_run_result))?;
            log::warning("Your call has not been executed.")?;
            log::warning(format!(
                    "To submit the transaction and execute the call on chain, add {} flag to the command.",
                    "-x/--execute"
            ))?;
        }
        else{
            let weight_limit;
            if self.gas_limit.is_some() && self.proof_size.is_some() {
                weight_limit = Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap());
            } else {
                let mut spinner = cliclack::spinner();
                spinner.start("Doing a dry run to estimate the gas...");
                weight_limit = match dry_run_gas_estimate_call(&call_exec).await {
                    Ok(w) => {
                        log::info(format!("Gas limit {:?}", w))?;
                        w
                    },
                    Err(e) => {
                        spinner.error(format!("{e}"));
                        outro_cancel("Deployment failed.")?;
                        return Ok(());
                    },
                };
            }
            let mut spinner = cliclack::spinner();
            spinner.start("Calling the contract...");

            let call_result = call_smart_contract(call_exec, weight_limit, token_metadata)
                .await
                .map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

            log::info(call_result)?;
        }

		outro("Call completed successfully!")?;
		Ok(())
	}

    async fn set_up_call(
		&self,
        token_metadata: TokenMetadata
	) -> anyhow::Result<CallExec<DefaultConfig, DefaultEnvironment, Keypair>> {
		// If the user specify a path (not current directory) have to manually add Cargo.toml here
		// or ask to the user the specific path
		let manifest_path;
		if self.path.is_some() {
			let full_path: PathBuf = PathBuf::from(
				self.path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml",
			);
			manifest_path = ManifestPath::try_from(Some(full_path))?;
		} else {
			manifest_path = ManifestPath::try_from(self.path.as_ref())?;
		}


		let signer = create_signer(&self.suri)?;
		let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
			.manifest_path(Some(manifest_path))
			.url(self.url.clone())
			.done();

		let call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
			CallCommandBuilder::new(self.contract.clone(), &self.message, extrinsic_opts)
				.args(self.args.clone())
				.value(self.value.denominate_balance(&token_metadata)?)
				.gas_limit(self.gas_limit)
				.proof_size(self.proof_size)
				.done()
				.await?;
		return Ok(call_exec);
	}
}
