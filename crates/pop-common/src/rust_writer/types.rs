// SPDX-License-Identifier: GPL-3.0

use syn::{Expr, Ident, ImplItem, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum DefaultConfigType {
	Default { type_default_impl: ImplItem },
	NoDefault,
	NoDefaultBounds { type_default_impl: ImplItem },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RuntimeUsedMacro {
	Runtime,
	ConstructRuntime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterTypes {
	pub ident: Ident,
	pub type_: Type,
	pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Preserver {
	lookup: String,
	inner: Option<Box<Preserver>>,
}

impl Preserver {
	pub(crate) fn new(lookup: &str) -> Self {
		Self { lookup: lookup.to_owned(), inner: None }
	}

	pub(crate) fn add_inners(&mut self, lookups: Vec<&str>) {
		let mut current = self;
		for lookup in lookups {
			current.inner = Some(Box::new(Self::new(lookup)));
			current = current.inner.as_mut().expect("Inner is Some due to the previous line; qed");
		}
	}

	pub(crate) fn lookup(&self) -> &str {
		&self.lookup
	}

	pub(crate) fn take_inner(&mut self) -> Option<Box<Preserver>> {
		self.inner.take()
	}
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DelimitersCount {
	counts: [u8; 6],
}

impl DelimitersCount {
	pub(crate) fn new() -> Self {
		Self { counts: [0; 6] }
	}

	pub(crate) fn is_complete(&self) -> bool {
		self.counts[0] == self.counts[1] && // `{` and `}`
        self.counts[2] == self.counts[3] && // `(` and `)`
        self.counts[4] == self.counts[5] // `[` and `]`
	}

	pub(crate) fn count(&mut self, line: &str) {
		self.counts[0] += line.matches('{').count() as u8;
		self.counts[1] += line.matches('}').count() as u8;
		self.counts[2] += line.matches('(').count() as u8;
		self.counts[3] += line.matches(')').count() as u8;
		self.counts[4] += line.matches('[').count() as u8;
		self.counts[5] += line.matches(']').count() as u8;
	}
}
