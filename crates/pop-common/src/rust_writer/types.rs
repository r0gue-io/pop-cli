// SPDX-License-Identifier: GPL-3.0

use syn::{Expr, ImplItem, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum DefaultConfigType {
	Default { type_default_impl: ImplItem },
	NoDefault,
	NoDefaultBounds { type_default_impl: ImplItem },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeUsedMacro {
	Runtime,
	ConstructRuntime,
	NotFound,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterTypes {
	pub ident: String,
	pub type_: Type,
	pub value: Expr,
}
