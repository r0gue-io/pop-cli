use anyhow::Result;
use clap::Args;

/// Launch a local fork of live-deployed contracts.
#[derive(Args)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct ForkContractCommand {}

impl ForkContractCommand {
	pub(crate) fn execute(self) -> Result<()> {
		Ok(())
	}
}
