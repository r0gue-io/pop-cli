// SPDX-License-Identifier: GPL-3.0

use crate::output::OutputMode;

/// Runs a duct expression, redirecting stdout to stderr in JSON mode
/// so that subprocess output does not pollute the JSON envelope on stdout.
#[allow(dead_code)]
pub(crate) fn run_external(
	expr: duct::Expression,
	output_mode: OutputMode,
) -> Result<(), anyhow::Error> {
	match output_mode {
		OutputMode::Human => expr.run()?,
		OutputMode::Json => expr.stdout_to_stderr().run()?,
	};
	Ok(())
}
