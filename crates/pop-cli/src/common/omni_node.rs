use crate::{
	cli::traits::Cli,
	common::binary::{BinaryGenerator, check_and_prompt},
	impl_binary_generator,
};
use pop_chains::omni_node::{PolkadotOmniNodeCli, polkadot_omni_node_generator};
use pop_common::sourcing::traits::enums::Source;
use std::path::{Path, PathBuf};

impl_binary_generator!(PolkadotOmniNodeGenerator, polkadot_omni_node_generator);

/// Sources and manages the polkadot-omni-node binary, handling download and installation if needed.
///
/// # Arguments
///
/// * `cli` - Mutable reference implementing the Cli trait for user interaction
/// * `cache_path` - Path where the binary should be cached
/// * `skip_confirm` - Whether to skip confirmation prompts during installation
///
/// # Returns
///
/// * `anyhow::Result<PathBuf>` - Path to the installed binary on success, or an error
pub async fn source_polkadot_omni_node_binary(
	cli: &mut impl Cli,
	spinner: &crate::cli::Spinner,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	check_and_prompt::<PolkadotOmniNodeGenerator>(
		cli,
		spinner,
		PolkadotOmniNodeCli::PolkadotOmniNode.binary(),
		cache_path,
		skip_confirm,
	)
	.await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;

	#[tokio::test]
	async fn source_polkadot_omni_node_binary_works() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir()?;
		let binary_name = PolkadotOmniNodeCli::PolkadotOmniNode.binary();
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {binary_name} binary is not found."))
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning(format!("⚠️ The {binary_name} binary is not found."));

		let node_path = source_polkadot_omni_node_binary(
			&mut cli,
			&crate::cli::Spinner::Mock,
			cache_path.path(),
			false,
		)
		.await?;

		// Binary path should start with cache path + binary name
		assert!(
			node_path
				.to_str()
				.unwrap()
				.starts_with(cache_path.path().join(binary_name).to_str().unwrap())
		);
		cli.verify()
	}

	#[tokio::test]
	async fn source_polkadot_omni_node_binary_handles_skip_confirm() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir()?;
		let binary_name = PolkadotOmniNodeCli::PolkadotOmniNode.binary();
		let mut cli =
			MockCli::new().expect_warning(format!("⚠️ The {binary_name} binary is not found."));

		let node_path = source_polkadot_omni_node_binary(
			&mut cli,
			&crate::cli::Spinner::Mock,
			cache_path.path(),
			true,
		)
		.await?;

		// Binary path should start with cache path + binary name
		assert!(
			node_path
				.to_str()
				.unwrap()
				.starts_with(cache_path.path().join(binary_name).to_str().unwrap())
		);
		cli.verify()
	}
}
