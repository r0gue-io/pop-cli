use std::path::PathBuf;

use clap::Args;
use cliclack::{clear_screen, intro, outro};
use pop_contracts::{audit_smart_contract};

use crate::style::style;

#[derive(Args)]
pub(crate) struct AuditContractCommand {
	#[arg(short = 'p', long, help = "Path for the contract project [default: current directory]")]
	path: Option<PathBuf>,
}

impl AuditContractCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
        intro(format!(
            "{}: Auditing the Smart Contract",
            style(" Pop CLI ").black().on_magenta()
        ))?;
        audit_smart_contract(&self.path)?;
        outro("Auditing complete")?;
		Ok(())
	}
}
