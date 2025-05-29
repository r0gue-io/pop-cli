use pop_common::{
	sourcing::{
		traits::{Source as _, TryInto},
		Binary, Source,
	},
	Error,
};
use std::{
	fs::File,
	path::PathBuf,
	process::{Child, Command, Stdio},
	time::Duration,
};
use strum_macros::{EnumProperty, VariantArray};
use tokio::time::sleep;

const STARTUP: Duration = Duration::from_millis(20_000);

/// A supported chain.
#[derive(Debug, EnumProperty, PartialEq, VariantArray)]
pub(super) enum NodeBinary {
	/// Minimal Substrate node configured for smart contracts via pallet-contracts.
	#[strum(props(
		Repository = "https://github.com/paritytech/polkadot-sdk-minimal-template",
		Binary = "minimal-template-node",
		TagFormat = "{tag}",
		Fallback = "v0.0.2"
	))]
	MinimalTemplate,
}

impl pop_common::sourcing::traits::Source for NodeBinary {}

impl TryInto for NodeBinary {
	/// Attempt the conversion.
	///
	/// # Arguments
	/// * `latest` - If applicable, some specifier used to determine the latest source.
	fn try_into(&self, _: Option<String>, latest: Option<String>) -> Result<Source, Error> {
		let repository = self.repository();
		let binary_name = self.binary();
		let fallback = self.fallback();
		Ok(match self {
			&NodeBinary::MinimalTemplate => Source::Url {
				url: format!(
					"{}/releases/download/{}/{}",
					repository,
					latest.unwrap_or(fallback.to_string()),
					binary_name
				),
				name: binary_name.to_string(),
			},
		})
	}
}

/// Retrieves the latest release of the minimal node binary, resolves its version, and constructs
/// a `Binary::Source` with the specified cache path.
///
/// # Arguments
/// * `cache` -  The cache directory path.
/// * `version` - The specific version used for the polkadot-sdk-minimal node (`None` will use the
///   latest available version).
pub async fn minimal_node_generator(
	cache: PathBuf,
	version: Option<&str>,
) -> Result<Binary, Error> {
	let chain = &NodeBinary::MinimalTemplate;
	let name = chain.binary();
	let releases = chain.releases().await?;
	let latest = version.is_none().then(|| releases.first().map(|v| v.to_string())).flatten();
	let binary = Binary::Source {
		name: name.to_string(),
		source: TryInto::try_into(chain, None, latest)?,
		cache: cache.to_path_buf(),
	};
	Ok(binary)
}

pub async fn run_minimal_node(
	binary_path: PathBuf,
	output: Option<&File>,
	port: u16,
) -> Result<Child, Error> {
	let mut command = Command::new(binary_path);
	command.arg("-linfo");
	command.arg(format!("--rpc-port={}", port));
	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}
	let process = command.spawn()?;
	// Wait until the node is ready
	sleep(STARTUP).await;
	Ok(process)
}

#[cfg(test)]
mod tests {
	use std::env::current_dir;

	use pop_common::{find_free_port, sourcing::github_binary_url};

	use super::*;

	#[tokio::test]
	async fn minimal_node_generator_works() -> anyhow::Result<()> {
		let expected = NodeBinary::MinimalTemplate;
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = temp_dir.path().join("cache");
		let version = "v0.0.2";
		let binary = minimal_node_generator(cache, Some(version)).await?;

		let url = github_binary_url(
			"paritytech/polkadot-sdk-minimal-template",
			"v0.0.2",
			"minimal-template-node",
		);
		assert!(matches!(binary, Binary::Source { name, source, cache}
			if name == expected.binary()  &&
				source == Source::Url {
					name: "minimal-template-node".to_string(),
					url: url.to_string()
				}
				&&
			cache == cache
		));
		Ok(())
	}

	// #[ignore = "Works fine locally but is causing issues when running tests in parallel in the CI
	// environment."]
	#[tokio::test]
	async fn run_minimal_node_works() -> Result<(), Error> {
		let random_port = find_free_port(None);
		let localhost_url = format!("ws://127.0.0.1:{}", random_port);
		let _local_url = url::Url::parse(&localhost_url)?;

		// let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_dir = current_dir()?;
		let cache = temp_dir.join("");

		let version = "v0.0.2";
		let binary = minimal_node_generator(cache.clone(), Some(version)).await?;
		binary.source(false, &(), true).await?;
		let process = run_minimal_node(binary.path(), None, 9947).await?;

		// Check if the node is alive
		assert!(cache.join("minimal-template-node-v0.0.2").exists());
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;

		Ok(())
	}
}
