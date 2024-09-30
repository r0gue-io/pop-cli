// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum CrateDependencie {
	External { version: String },
	Local { local_crate_path: PathBuf },
	Workspace,
}
