// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

use crate::utils::helpers::write_to_file;
use askama::Template;

#[derive(Template)]
#[template(path = "pallet/Cargo.templ", escape = "none")]
pub(crate) struct PalletCargoToml {
	pub(crate) name: String,
	pub(crate) authors: String,
	pub(crate) description: String,
}
#[derive(Template)]
#[template(path = "pallet/src/benchmarking.rs.templ", escape = "none")]
pub(crate) struct PalletBenchmarking {}
#[derive(Template)]
#[template(path = "pallet/src/lib.rs.templ", escape = "none")]
pub(crate) struct PalletLib {
    pub(crate) name: String,
}
#[derive(Template)]
#[template(path = "pallet/src/mock.rs.templ", escape = "none")]
pub(crate) struct PalletMock {
	pub(crate) name: String,
}
#[derive(Template)]
#[template(path = "pallet/src/tests.rs.templ", escape = "none")]
pub(crate) struct PalletTests {
	pub(crate) name: String,
}

#[derive(Template)]
#[template(path = "pallet/src/weights.rs.templ", escape = "none")]
pub(crate) struct PalletWeights {}

pub trait PalletItem {
	/// Render and Write to file, root is the path to the pallet
	fn execute(&self, root: &PathBuf) -> anyhow::Result<()>;
}

macro_rules! generate_pallet_item {
	($item:ty, $filename:expr) => {
		impl PalletItem for $item {
			fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
				let rendered = self.render()?;
				let _ = write_to_file(&root.join($filename), &rendered);
				Ok(())
			}
		}
	};
}

generate_pallet_item!(PalletTests, "src/tests.rs");
generate_pallet_item!(PalletMock, "src/mock.rs");
generate_pallet_item!(PalletLib, "src/lib.rs");
generate_pallet_item!(PalletBenchmarking, "src/benchmarking.rs");
generate_pallet_item!(PalletCargoToml, "Cargo.toml");
generate_pallet_item!(PalletWeights, "src/weights.rs");
