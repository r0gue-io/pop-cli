// SPDX-License-Identifier: GPL-3.0

use crate::{
	manifest::{
		add_crate_to_dependencies, find_crate_manifest, find_crate_name,
		find_pallet_runtime_impl_path, types::CrateDependencie,
	},
	Error,
};
use prettyplease::unparse;
use std::{fs, path::Path};
use syn::Type;

mod expand;
mod helpers;
mod parse;
pub mod types;

pub fn update_config_trait(
	file_path: &Path,
	type_name: &str,
	trait_bounds: Vec<&str>,
	default_config: types::DefaultConfigType,
) -> Result<(), Error> {
	let mut ast = helpers::preserve_and_parse(fs::read_to_string(file_path)?, vec![])?;

	// Expand the config trait
	expand::expand_pallet_config_trait(&mut ast, default_config, type_name, trait_bounds);
	let generated_code = helpers::resolve_preserved(unparse(&ast));

	fs::write(file_path, &generated_code).map_err(|_| {
		Error::WriteError(format!("Path :{}", file_path.to_str().unwrap_or("Invalid UTF-8 path")))
	})?;

	Ok(())
}

pub fn add_type_to_runtimes(
	pallet_path: &Path,
	type_name: &str,
	runtime_value: &str,
	runtime_impl_path: Option<&Path>,
) -> Result<(), Error> {
	fn do_add_type_to_runtime(
		file_content: &str,
		file_path: &Path,
		pallet_manifest_path: &Path,
		type_name: &str,
		runtime_value: &str,
	) -> Result<(), Error> {
		let mut ast = helpers::preserve_and_parse(file_content.to_string(), vec![])?;

		let pallet_name = find_crate_name(pallet_manifest_path)?.replace("-", "_");

		expand::expand_runtime_add_type_to_impl_block(
			&mut ast,
			type_name,
			runtime_value,
			&pallet_name,
		);

		let generated_code = helpers::resolve_preserved(unparse(&ast));

		fs::write(file_path, generated_code).map_err(|_| {
			Error::WriteError(format!(
				"Path :{}",
				file_path.to_str().unwrap_or("Invalid UTF-8 path")
			))
		})?;

		Ok(())
	}

	let src = pallet_path.join("src");
	let pallet_manifest_path = pallet_path.join("Cargo.toml");
	// All pallets should have a mock runtime, so we add the type to it.
	let mock_path = src.join("mock.rs");
	let mock_content = fs::read_to_string(&mock_path)?;
	do_add_type_to_runtime(
		&mock_content,
		&mock_path,
		&pallet_manifest_path,
		type_name,
		runtime_value,
	)?;

	// If the pallet is contained inside a runtime add the type to that runtime as well
	if let Some(runtime_impl_path) = runtime_impl_path
		.map(|inner| inner.to_path_buf())
		.or_else(|| find_pallet_runtime_impl_path(pallet_path))
	{
		let runtime_impl_content = fs::read_to_string(&runtime_impl_path)?;
		do_add_type_to_runtime(
			&runtime_impl_content,
			&runtime_impl_path,
			&pallet_manifest_path,
			type_name,
			runtime_value,
		)?;
	}
	Ok(())
}

pub fn add_type_to_config_preludes(
	file_path: &Path,
	type_name: &str,
	default_value: &str,
) -> Result<(), Error> {
	let mut ast = helpers::preserve_and_parse(fs::read_to_string(file_path)?, vec![])?;

	// Expand the config_preludes
	expand::expand_pallet_config_preludes(&mut ast, type_name, default_value);

	let generated_code = helpers::resolve_preserved(unparse(&ast));

	fs::write(file_path, generated_code).map_err(|_| {
		Error::WriteError(format!("Path :{}", file_path.to_str().unwrap_or("Invalid UTF-8 path")))
	})?;

	Ok(())
}

pub fn add_pallet_to_runtime_module(
	pallet_name: &str,
	runtime_lib_path: &Path,
	pallet_dependencie_type: CrateDependencie,
) -> Result<(), Error> {
	// Find the pallet name and the pallet item to be added to the runtime. If the pallet_name is
	// behind the form pallet-some-thing, pallet_item becomes Something.
	let pallet_item = helpers::capitalize_str(
		&pallet_name
			.split("pallet-")
			.last()
			.ok_or(Error::Config(
				"Pallet crates are supposed to be called pallet-something.".to_string(),
			))?
			.replace("-", ""),
	);

	// As the runtime may be constructed with construc_runtime!, we have to avoid preserving that
	// macro with comments
	let mut ast = helpers::preserve_and_parse(
		fs::read_to_string(runtime_lib_path)?,
		vec!["construct_runtime"],
	)?;

	// Parse the runtime to find which of the runtime macros is being used and the highest
	// pallet index used (if needed).
	let (highest_index, used_macro) =
		parse::find_highest_pallet_index_and_runtime_macro_version(&ast);
	// Expand the ast with the new pallet. pallet-some-thing becomes pallet_some_thing in the code
	expand::expand_runtime_add_pallet(
		&mut ast,
		highest_index,
		used_macro,
		&pallet_name.replace("-", "_"),
		&pallet_item,
	);

	let generated_code = helpers::resolve_preserved(unparse(&ast));

	fs::write(runtime_lib_path, generated_code).map_err(|_| {
		Error::WriteError(format!(
			"Path :{}",
			runtime_lib_path.to_str().unwrap_or("Invalid UTF-8 path")
		))
	})?;

	// Update the crate's manifest to add the pallet crate
	let runtime_manifest = find_crate_manifest(runtime_lib_path)
		.expect("Runtime is a crate, so it contains a manifest; qed;");

	add_crate_to_dependencies(&runtime_manifest, pallet_name, pallet_dependencie_type)?;

	Ok(())
}

pub fn add_pallet_impl_block_to_runtime(
	pallet_name: &str,
	runtime_impl_path: &Path,
	parameter_types: Vec<types::ParameterTypes>,
	types: Vec<String>,
	values: Vec<Type>,
	default_config: bool,
) -> Result<(), Error> {
	let mut ast = helpers::preserve_and_parse(fs::read_to_string(runtime_impl_path)?, vec![])?;

	// Expand the runtime to add the impl_block
	expand::expand_runtime_add_impl_block(
		&mut ast,
		&pallet_name.replace("-", "_"),
		parameter_types,
		types,
		values,
		default_config,
	);

	let generated_code = helpers::resolve_preserved(unparse(&ast));

	fs::write(runtime_impl_path, generated_code).map_err(|_| {
		Error::WriteError(format!(
			"Path :{}",
			runtime_impl_path.to_str().unwrap_or("Invalid UTF-8 path")
		))
	})?;
	Ok(())
}
