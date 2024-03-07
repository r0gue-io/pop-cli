use std::path::{Path, PathBuf};

use askama::Template;

use crate::helpers::write_to_file;

#[derive(Template)]
#[template(path = "vanilla/chain_spec.templ", escape = "none")]
pub(crate) struct ChainSpec {
	pub(crate) token_symbol: String,
	pub(crate) decimals: u8,
	pub(crate) initial_endowment: String,
}

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
pub(crate) struct PalletLib {}
#[derive(Template)]
#[template(path = "pallet/src/mock.rs.templ", escape = "none")]
pub(crate) struct PalletMock {
	pub(crate) module: String,
}
#[derive(Template)]
#[template(path = "pallet/src/tests.rs.templ", escape = "none")]
pub(crate) struct PalletTests {
	pub(crate) module: String,
}

// todo : generate directory structure
// todo : This is only for development
#[allow(unused)]
pub fn generate() {
	let cs = ChainSpec {
		token_symbol: "DOT".to_owned(),
		decimals: 10,
		initial_endowment: "1u64 << 15".to_owned(),
	};
	let rendered = cs.render().unwrap();
	write_to_file(Path::new("src/x.rs"), &rendered);
}

pub trait PalletItem {
	/// Render and Write to file, root is the path to the pallet
	fn execute(&self, root: &PathBuf) -> anyhow::Result<()>;
}

macro_rules! generate_pallet_item {
	($item:ty, $filename:expr) => {
		impl PalletItem for $item {
			fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
				let rendered = self.render()?;
				write_to_file(&root.join($filename), &rendered);
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
