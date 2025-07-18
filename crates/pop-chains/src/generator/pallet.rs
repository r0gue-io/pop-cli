// SPDX-License-Identifier: GPL-3.0

use std::path::Path;

use crate::{
	utils::helpers::write_to_file, TemplatePalletConfigCommonTypes, TemplatePalletStorageTypes,
};
use askama::Template;

mod filters {
	/// This filter is used to determine if a element is present in a `Vec`
	pub fn contains<T: PartialEq>(vec: &[T], element: T) -> ::askama::Result<bool> {
		Ok(vec.contains(&element))
	}
}

#[derive(Template)]
#[template(path = "pallet/Cargo.templ", escape = "none")]
pub(crate) struct PalletCargoToml {
	pub(crate) name: String,
	pub(crate) authors: String,
	pub(crate) description: String,
	// A bool indicating if the pallet has been generated inside a workspace
	pub(crate) pallet_in_workspace: bool,
	// Some common types are used to couple our pallet with a well known one, then adding this type
	// here is useful to design Cargo.toml. This pallets should be added as dev-dependencies to
	// construct the mock runtime
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
}

// Templates for simple mode
#[derive(Template)]
#[template(path = "pallet/simple_mode/src/lib.rs.templ", escape = "none")]
pub(crate) struct PalletSimpleLib {
	pub(crate) name: String,
}

#[derive(Template)]
#[template(path = "pallet/simple_mode/src/tests.rs.templ", escape = "none")]
pub(crate) struct PalletSimpleTests {
	pub(crate) name: String,
}

#[derive(Template)]
#[template(path = "pallet/simple_mode/src/mock.rs.templ", escape = "none")]
pub(crate) struct PalletSimpleMock {
	pub(crate) name: String,
}

#[derive(Template)]
#[template(path = "pallet/simple_mode/src/benchmarking.rs.templ", escape = "none")]
pub(crate) struct PalletSimpleBenchmarking {}

#[derive(Template)]
#[template(path = "pallet/simple_mode/src/weights.rs.templ", escape = "none")]
pub(crate) struct PalletWeights {}

// Templates for advanced mode
#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/lib.rs.templ", escape = "none")]
pub(crate) struct PalletAdvancedLib {
	pub(crate) name: String,
	pub(crate) pallet_default_config: bool,
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_storage: Vec<TemplatePalletStorageTypes>,
	pub(crate) pallet_genesis: bool,
	pub(crate) pallet_custom_origin: bool,
}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/tests.rs.templ", escape = "none")]
pub(crate) struct PalletAdvancedTests {}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/mock.rs.templ", escape = "none")]
pub(crate) struct PalletAdvancedMock {
	pub(crate) name: String,
	pub(crate) pallet_default_config: bool,
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_custom_origin: bool,
}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/benchmarking.rs.templ", escape = "none")]
pub(crate) struct PalletAdvancedBenchmarking {}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/pallet_logic.rs.templ", escape = "none")]
pub(crate) struct PalletLogic {
	pub(crate) pallet_custom_origin: bool,
}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/config_preludes.rs.templ", escape = "none")]
pub(crate) struct PalletConfigPreludes {
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_custom_origin: bool,
}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/pallet_logic/try_state.rs.templ", escape = "none")]
pub(crate) struct PalletTryState {}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/pallet_logic/origin.rs.templ", escape = "none")]
pub(crate) struct PalletOrigin {}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/tests/utils.rs.templ", escape = "none")]
pub(crate) struct PalletTestsUtils {
	pub(crate) name: String,
}

#[derive(Template)]
#[template(path = "pallet/advanced_mode/src/types.rs.templ", escape = "none")]
pub(crate) struct PalletTypes {
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_storage: Vec<TemplatePalletStorageTypes>,
	pub(crate) pallet_custom_origin: bool,
}

pub trait PalletItem {
	/// Render and Write to file, root is the path to the pallet
	fn execute(&self, root: &Path) -> anyhow::Result<()>;
}

macro_rules! generate_pallet_item {
	($item:ty, $filename:expr) => {
		impl PalletItem for $item {
			fn execute(&self, root: &Path) -> anyhow::Result<()> {
				let rendered = self.render()?;
				let _ = write_to_file(&root.join($filename), &rendered);
				Ok(())
			}
		}
	};
}

generate_pallet_item!(PalletCargoToml, "Cargo.toml");
generate_pallet_item!(PalletSimpleLib, "src/lib.rs");
generate_pallet_item!(PalletSimpleTests, "src/tests.rs");
generate_pallet_item!(PalletSimpleMock, "src/mock.rs");
generate_pallet_item!(PalletSimpleBenchmarking, "src/benchmarking.rs");
generate_pallet_item!(PalletWeights, "src/weights.rs");
generate_pallet_item!(PalletAdvancedLib, "src/lib.rs");
generate_pallet_item!(PalletAdvancedTests, "src/tests.rs");
generate_pallet_item!(PalletAdvancedMock, "src/mock.rs");
generate_pallet_item!(PalletAdvancedBenchmarking, "src/benchmarking.rs");
generate_pallet_item!(PalletLogic, "src/pallet_logic.rs");
generate_pallet_item!(PalletConfigPreludes, "src/config_preludes.rs");
generate_pallet_item!(PalletTryState, "src/pallet_logic/try_state.rs");
generate_pallet_item!(PalletOrigin, "src/pallet_logic/origin.rs");
generate_pallet_item!(PalletTestsUtils, "src/tests/utils.rs");
generate_pallet_item!(PalletTypes, "src/types.rs");
