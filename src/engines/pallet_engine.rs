// Copyright (C) R0GUE IO LTD.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	engines::generator::PalletItem,
	helpers::{resolve_pallet_path, sanitize},
};
use std::{fs, path::PathBuf};

pub fn create_pallet_template(
	path: Option<String>,
	config: TemplatePalletConfig,
) -> anyhow::Result<()> {
	let target = resolve_pallet_path(path);
	let pallet_name = config.name.clone();
	let pallet_path = target.join(pallet_name.clone());
	sanitize(&pallet_path)?;
	generate_pallet_structure(&target, &pallet_name)?;

	render_pallet(pallet_name, config, &pallet_path)?;
	Ok(())
}
pub struct TemplatePalletConfig {
	pub(crate) name: String,
	pub(crate) authors: String,
	pub(crate) description: String,
}
/// Generate a pallet folder and file structure
fn generate_pallet_structure(target: &PathBuf, pallet_name: &str) -> anyhow::Result<()> {
	use fs::{create_dir, File};
	let (pallet, src) = (target.join(pallet_name), target.join(pallet_name.to_string() + "/src"));
	create_dir(&pallet)?;
	create_dir(&src)?;
	File::create(format!("{}/Cargo.toml", pallet.display()))?;
	File::create(format!("{}/lib.rs", src.display()))?;
	File::create(format!("{}/benchmarking.rs", src.display()))?;
	File::create(format!("{}/tests.rs", src.display()))?;
	File::create(format!("{}/mock.rs", src.display()))?;
	Ok(())
}

fn render_pallet(
	pallet_name: String,
	config: TemplatePalletConfig,
	pallet_path: &PathBuf,
) -> anyhow::Result<()> {
	let pallet_name = pallet_name.replace('-', "_");
	use crate::engines::generator::{
		PalletBenchmarking, PalletCargoToml, PalletLib, PalletMock, PalletTests,
	};
	// Todo `module` must be of the form Template if pallet_name : `pallet_template`
	let pallet: Vec<Box<dyn PalletItem>> = vec![
		Box::new(PalletCargoToml {
			name: pallet_name.clone(),
			authors: config.authors,
			description: config.description,
		}),
		Box::new(PalletLib {}),
		Box::new(PalletBenchmarking {}),
		Box::new(PalletMock { module: pallet_name.clone() }),
		Box::new(PalletTests { module: pallet_name }),
	];
	for item in pallet {
		item.execute(pallet_path)?;
	}
	Ok(())
}
