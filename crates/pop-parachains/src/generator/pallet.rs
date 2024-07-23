// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

use crate::{
	utils::helpers::write_to_file, TemplatePalletConfigCommonTypes,
	TemplatePalletConfigTypesDefault, TemplatePalletConfigTypesMetadata,
	TemplatePalletStorageTypes,
};
use askama::Template;

mod filters {
	/// This filter is used to determine if a element is present in a `Vec`
	pub fn contains<T: PartialEq>(vec: &Vec<T>, element: T) -> ::askama::Result<bool> {
		Ok(vec.contains(&element))
	}
}

#[derive(Template)]
#[template(path = "pallet/Cargo.templ", escape = "none")]
pub(crate) struct PalletCargoToml {
	pub(crate) name: String,
	pub(crate) authors: String,
	pub(crate) description: String,
	// Some common types are used to couple our pallet with a well known one, then adding this type
	// here is useful to design Cargo.toml. This pallets should be added as dev-dependencies to
	// construct the mock runtime
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
}
#[derive(Template)]
#[template(path = "pallet/src/benchmarking.rs.templ", escape = "none")]
pub(crate) struct PalletBenchmarking {}
#[derive(Template)]
#[template(path = "pallet/src/lib.rs.templ", escape = "none")]
pub(crate) struct PalletLib {
	pub(crate) name: String,
	pub(crate) pallet_default_config: bool,
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_config_types:
		Vec<(TemplatePalletConfigTypesMetadata, TemplatePalletConfigTypesDefault, String)>,
	pub(crate) pallet_storage: Vec<(TemplatePalletStorageTypes, String)>,
	pub(crate) pallet_genesis: bool,
	pub(crate) pallet_custom_internal_origin_variants: Vec<String>,
}
#[derive(Template)]
#[template(path = "pallet/src/pallet_logic.rs.templ", escape = "none")]
pub(crate) struct PalletLogic {
	pub(crate) pallet_custom_internal_origin_variants: Vec<String>,
}
#[derive(Template)]
#[template(path = "pallet/src/config_preludes.rs.templ", escape = "none")]
pub(crate) struct PalletConfigPreludes {
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_config_types:
		Vec<(TemplatePalletConfigTypesMetadata, TemplatePalletConfigTypesDefault, String)>,
}
#[derive(Template)]
#[template(path = "pallet/src/pallet_logic/try_state.rs.templ", escape = "none")]
pub(crate) struct PalletTryState {}
#[derive(Template)]
#[template(path = "pallet/src/pallet_logic/origin.rs.templ", escape = "none")]
pub(crate) struct PalletOrigin {
	pub(crate) pallet_custom_internal_origin_variants: Vec<String>,
}
#[derive(Template)]
#[template(path = "pallet/src/mock.rs.templ", escape = "none")]
pub(crate) struct PalletMock {
	pub(crate) name: String,
	pub(crate) pallet_default_config: bool,
	pub(crate) pallet_common_types: Vec<TemplatePalletConfigCommonTypes>,
	pub(crate) pallet_config_types:
		Vec<(TemplatePalletConfigTypesMetadata, TemplatePalletConfigTypesDefault, String)>,
}
#[derive(Template)]
#[template(path = "pallet/src/tests.rs.templ", escape = "none")]
pub(crate) struct PalletTests {}
#[derive(Template)]
#[template(path = "pallet/src/tests/utils.rs.templ", escape = "none")]
pub(crate) struct PalletTestsUtils {
	pub(crate) name: String,
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
				let _ = write_to_file(&root.join($filename), &rendered);
				Ok(())
			}
		}
	};
}

generate_pallet_item!(PalletTests, "src/tests.rs");
generate_pallet_item!(PalletTestsUtils, "src/tests/utils.rs");
generate_pallet_item!(PalletMock, "src/mock.rs");
generate_pallet_item!(PalletLib, "src/lib.rs");
generate_pallet_item!(PalletLogic, "src/pallet_logic.rs");
generate_pallet_item!(PalletConfigPreludes, "src/config_preludes.rs");
generate_pallet_item!(PalletTryState, "src/pallet_logic/try_state.rs");
generate_pallet_item!(PalletOrigin, "src/pallet_logic/origin.rs");
generate_pallet_item!(PalletBenchmarking, "src/benchmarking.rs");
generate_pallet_item!(PalletCargoToml, "Cargo.toml");
