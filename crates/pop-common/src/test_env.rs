// SPDX-License-Identifier: GPL-3.0

use crate::{
	find_free_port,
	polkadot_sdk::sort_by_latest_semantic_version,
	set_executable_permission,
	sourcing::{ArchiveFileSpec, Binary, GitHub::ReleaseArchive, Source::GitHub},
	Error,
};

use std::{
	env::consts::{ARCH, OS},
	process::{Child, Command, Stdio},
	time::Duration,
};
use tempfile::TempDir;
use tokio::{sync::OnceCell as AsyncOnceCell, time::sleep};

const STARTUP: Duration = Duration::from_millis(20_000);

pub struct TestNode {
	_child: Child,
	_temp_dir: TempDir,
	ws_url: String,
}

impl TestNode {
	pub async fn new() -> anyhow::Result<Self> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let random_port = find_free_port(None);
		let cache = temp_dir.path().join("");

		let binary = Binary::Source {
			name: "ink-node".to_string(),
			source: GitHub(ReleaseArchive {
				owner: "use-ink".into(),
				repository: "ink-node".into(),
				tag: None,
				tag_pattern: Some("v{version}".into()),
				prerelease: false,
				version_comparator: sort_by_latest_semantic_version,
				fallback: "v0.43.0".to_string(),
				archive: archive_name_by_target()?,
				contents: release_directory_by_target("ink-node")?,
				latest: None,
			})
			.into(),
			cache: cache.to_path_buf(),
		};
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;

		let mut command = Command::new(binary.path());
		command.arg("-linfo,runtime::contracts=debug");
		command.arg(format!("--rpc-port={}", random_port));
		let child = command.spawn()?;
		command.stderr(Stdio::null());

		// Wait until the node is ready
		sleep(STARTUP).await;

		let ws_url = format!("ws://127.0.0.1:{random_port}");
		println!("{:?}", ws_url);

		Ok(Self { _child: child, _temp_dir: temp_dir, ws_url })
	}

	pub fn ws_url(&self) -> &str {
		&self.ws_url
	}
}

fn archive_name_by_target() -> Result<String, Error> {
	match OS {
		"macos" => Ok("ink-node-mac-universal.tar.gz".to_string()),
		"linux" => Ok("ink-node-linux.tar.gz".to_string()),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}

fn release_directory_by_target(binary: &str) -> Result<Vec<ArchiveFileSpec>, Error> {
	match OS {
		"macos" => Ok("ink-node-mac/ink-node"),
		"linux" => Ok("ink-node-linux/ink-node"),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
	.map(|name| vec![ArchiveFileSpec::new(name.into(), Some(binary.into()), true)])
}

static TEST_NODE: AsyncOnceCell<TestNode> = AsyncOnceCell::const_new();

/// Spawns and caches a single test node instance for all tests.
pub async fn ensure_test_node() -> &'static TestNode {
	TEST_NODE
		.get_or_try_init(TestNode::new)
		.await
		.expect("Failed to initialize test node")
}
