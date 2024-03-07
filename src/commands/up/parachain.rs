use crate::style::{style, Theme};
use clap::Args;
use cliclack::{clear_screen, confirm, intro, log, outro, outro_cancel, set_theme};

#[derive(Args)]
pub(crate) struct ZombienetCommand {
    /// The configuration file to be used. Only Zombienet configuration files are currently supported.
    #[arg(short, long)]
    file: String,
    /// The version of Polkadot to be used for the relay chain, as per the release tag (e.g. "v1.7.0").
    #[arg(short, long)]
    relay_chain: Option<String>,
    /// The version of Polkadot to be used for a system parachain, as per the release tag (e.g. "v1.7.0").
    #[arg(short, long)]
    system_parachain: Option<String>,
    /// The url of the git repository of a parachain to be used, with branch/release tag specified as #fragment (e.g. 'https://github.com/org/repository#tag'). A specific binary name can also be optionally specified via query string parameter (e.g. 'https://github.com/org/repository?binaryname#tag'), defaulting to the name of the repository when not specified.
    #[arg(short, long)]
    parachain: Option<Vec<String>>,
}
impl ZombienetCommand {
    pub(crate) async fn execute(&self) -> anyhow::Result<()> {
        clear_screen()?;
        intro(format!(
            "{}: Deploy a parachain",
            style(" Pop CLI ").black().on_magenta()
        ))?;
        set_theme(Theme);
        // Parse arguments
        let cache = crate::cache()?;
        let mut zombienet = crate::parachains::zombienet::Zombienet::new(
            cache.clone(),
            &self.file,
            self.relay_chain.as_ref(),
            self.system_parachain.as_ref(),
            self.parachain.as_ref(),
        )
        .await?;
        // Check if any binaries need to be sourced
        let missing = zombienet.missing_binaries();
        if missing.len() > 0 {
            log::warning(format!(
                "The following missing binaries are required: {}",
                missing
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))?;
            if !confirm("Would you like to source them automatically now?").interact()? {
                outro_cancel("Cannot deploy parachain to local network until all required binaries are available.")?;
                return Ok(());
            }
            log::info(format!(
                "They will be cached at {}",
                &cache.to_str().unwrap()
            ))?;
            // Source binaries
            for binary in missing {
                let mut spinner = cliclack::spinner();
                spinner.start(format!("Sourcing {}...", binary.name));
                binary.source(&cache).await?;
                spinner.stop("Sourcing complete");
            }
        }
        // Finally spawn network and wait for signal to terminate
        log::info("Launching local network...")?;
        tracing_subscriber::fmt().init();
        match zombienet.spawn().await {
            Ok(_network) => {
                let mut spinner = cliclack::spinner();
                spinner.start("Local network launched - ctrl-c to terminate.");
                tokio::signal::ctrl_c().await?;
                outro("Done")?;
            }
            Err(e) => {
                outro_cancel(format!("Could not spawn network: {e}"))?;
            }
        }

        Ok(())
    }
}
