use anyhow::Result;
use clap::Args;

/// Launch a local fork of live running chains.
#[derive(Args)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct ForkChainCommand {}

impl ForkChainCommand {
	pub(crate) fn execute(self) -> Result<()> {
		Ok(())
	}
}
