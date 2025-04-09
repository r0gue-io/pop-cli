// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::Cli;
use anyhow::Result;

// Displays a message to the user, with formatting based on the success status.
#[allow(dead_code)]
pub(crate) fn display_message(message: &str, success: bool, cli: &mut impl Cli) -> Result<()> {
	if success {
		cli.outro(message)?;
	} else {
		cli.outro_cancel(message)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::display_message;
	use crate::cli::MockCli;
	use anyhow::Result;

	#[test]
	fn display_message_works() -> Result<()> {
		let mut cli = MockCli::new().expect_outro(&"Call completed successfully!");
		display_message("Call completed successfully!", true, &mut cli)?;
		cli.verify()?;
		let mut cli = MockCli::new().expect_outro_cancel("Call failed.");
		display_message("Call failed.", false, &mut cli)?;
		cli.verify()
	}
}
