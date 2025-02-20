// SPDX-License-Identifier: GPL-3.0

use crate::Error;

pub mod container_engine;
// Generates chain spec files for the parachain.
async fn generate_deterministic_runtime() -> Result<(), Error> {
    // format!("{engine} pull {image}:{tag}");
    Ok(())
}