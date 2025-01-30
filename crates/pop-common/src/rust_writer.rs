// SPDX-License-Identifier: GPL-3.0

use crate::{
	capitalize_str,
	manifest::{
		add_crate_to_dependencies, find_crate_manifest, find_crate_name,
		find_pallet_runtime_impl_path, types::CrateDependencie,
	},
	Error,
};
use prettyplease::unparse;
use proc_macro2::Span;
use std::{fs, path::Path};
use syn::{parse_str, Ident, ImplItem, ItemEnum, ItemUse, TraitBound, Type};

mod expand;
mod parse;
mod preserver;
#[cfg(test)]
mod tests;
pub mod types;

pub fn update_config_trait(
	file_path: &Path,
	type_name: Ident,
	trait_bounds: Vec<TraitBound>,
	default_config: &types::DefaultConfigType,
) -> Result<(), Error> {
	let mut preserver = types::Preserver::new("pub mod pallet");
	preserver.add_inners(vec!["pub trait Config"]);

	let mut ast = preserver::preserve_and_parse(fs::read_to_string(file_path)?, vec![preserver])?;

	// Expand the config trait
	expand::expand_pallet_config_trait(&mut ast, default_config, type_name, trait_bounds);
	let generated_code = preserver::resolve_preserved(unparse(&ast));

	fs::write(file_path, &generated_code).map_err(|_| {
		Error::WriteError(format!("Path :{}", file_path.to_str().unwrap_or("Invalid UTF-8 path")))
	})?;

	Ok(())
}

pub fn add_type_to_runtimes(
	pallet_path: &Path,
	type_name: Ident,
	runtime_value: Type,
	runtime_impl_path: Option<&Path>,
) -> Result<(), Error> {
	fn do_add_type_to_runtime(
		file_content: &str,
		file_path: &Path,
		pallet_manifest_path: &Path,
		type_name: Ident,
		runtime_value: Type,
	) -> Result<(), Error> {
		let pallet_name = find_crate_name(pallet_manifest_path)?.replace("-", "_");
		let preserver = types::Preserver::new(&format!("impl {}::Config", pallet_name));

		let mut ast = preserver::preserve_and_parse(file_content.to_string(), vec![preserver])?;

		expand::expand_runtime_add_type_to_impl_block(
			&mut ast,
			type_name,
			runtime_value,
			&pallet_name,
		);

		let generated_code = preserver::resolve_preserved(unparse(&ast));

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
		type_name.clone(),
		runtime_value.clone(),
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
	type_default_impl: ImplItem,
) -> Result<(), Error> {
	// Define preservers for the most common used struct names for default config. Both for
	// independent module file and module contained inside pallet
	let preserver_testchain_config =
		types::Preserver::new("impl DefaultConfig for TestDefaultConfig");
	let mut preserver_testchain_config_inside_pallet_mod = types::Preserver::new("pub mod pallet");
	preserver_testchain_config_inside_pallet_mod
		.add_inners(vec!["pub mod config_preludes", "impl DefaultConfig for TestDefaultConfig"]);

	let preserver_solochain_config =
		types::Preserver::new("impl DefaultConfig for SolochainDefaultConfig");
	let mut preserver_solochain_config_inside_pallet_mod = types::Preserver::new("pub mod pallet");
	preserver_solochain_config_inside_pallet_mod.add_inners(vec![
		"pub mod config_preludes",
		"impl DefaultConfig for SolochainDefaultConfig",
	]);

	let preserver_relaychain_config =
		types::Preserver::new("impl DefaultConfig for RelayChainDefaultConfig");
	let mut preserver_relaychain_config_inside_pallet_mod = types::Preserver::new("pub mod pallet");
	preserver_relaychain_config_inside_pallet_mod.add_inners(vec![
		"pub mod config_preludes",
		"impl DefaultConfig for RelayChainDefaultConfig",
	]);

	let preserver_parachain_config =
		types::Preserver::new("impl DefaultConfig for ParaChainDefaultConfig");
	let mut preserver_parachain_config_inside_pallet_mod = types::Preserver::new("pub mod pallet");
	preserver_parachain_config_inside_pallet_mod.add_inners(vec![
		"pub mod config_preludes",
		"impl DefaultConfig for ParaChainDefaultConfig",
	]);

	let mut ast = preserver::preserve_and_parse(
		fs::read_to_string(file_path)?,
		vec![
			preserver_testchain_config,
			preserver_testchain_config_inside_pallet_mod,
			preserver_solochain_config,
			preserver_solochain_config_inside_pallet_mod,
			preserver_relaychain_config,
			preserver_relaychain_config_inside_pallet_mod,
			preserver_parachain_config,
			preserver_parachain_config_inside_pallet_mod,
		],
	)?;

	// Expand the config_preludes
	expand::expand_pallet_config_preludes(&mut ast, type_default_impl);

	let generated_code = preserver::resolve_preserved(unparse(&ast));

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
	let preserver_construct_runtime = types::Preserver::new("construct_runtime!");
	let preserver_mod_runtime = types::Preserver::new("mod runtime");
	let mut ast = preserver::preserve_and_parse(
		fs::read_to_string(runtime_lib_path)?,
		vec![preserver_construct_runtime, preserver_mod_runtime],
	)?;

	// Parse the runtime to find which of the runtime macros is being used and the highest
	// pallet index used (if needed, otherwise 0).
	let used_macro = parse::find_used_runtime_macro(&ast)?;
	let mut highest_index = 0;
	if let types::RuntimeUsedMacro::Runtime = used_macro {
		highest_index = parse::find_highest_pallet_index(&ast)?;
	}

	// Find the pallet name and the pallet item to be added to the runtime. If the pallet_name is
	// behind the form pallet-some-thing, pallet_item becomes Something.
	let pallet_item = Ident::new(
		&capitalize_str(
			&pallet_name
				.split("pallet-")
				.last()
				.ok_or(Error::Config(
					"Pallet crates are supposed to be called pallet-something.".to_string(),
				))?
				.replace("-", ""),
		),
		Span::call_site(),
	);
	let pallet_name_type = parse_str::<Type>(&pallet_name.replace("-", "_"))?;

	// Expand the ast with the new pallet. pallet-some-thing becomes pallet_some_thing in the code
	expand::expand_runtime_add_pallet(
		&mut ast,
		highest_index,
		used_macro,
		pallet_name_type,
		pallet_item,
	);

	let generated_code = preserver::resolve_preserved(unparse(&ast));

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
	types: Vec<Ident>,
	values: Vec<Type>,
	default_config: bool,
) -> Result<(), Error> {
	// Nothing to preserve in this ast as this is a new impl block
	let mut ast = preserver::preserve_and_parse(fs::read_to_string(runtime_impl_path)?, vec![])?;
	let pallet_name_ident = Ident::new(&pallet_name.replace("-", "_"), Span::call_site());
	// Expand the runtime to add the impl_block
	expand::expand_runtime_add_impl_block(
		&mut ast,
		pallet_name_ident,
		parameter_types,
		default_config,
	);
	// Expand the block to add the types
	types.into_iter().zip(values).for_each(|(type_, value)| {
		expand::expand_runtime_add_type_to_impl_block(
			&mut ast,
			type_,
			value,
			&pallet_name.replace("-", "_"),
		)
	});

	let generated_code = preserver::resolve_preserved(unparse(&ast));

	fs::write(runtime_impl_path, generated_code).map_err(|_| {
		Error::WriteError(format!(
			"Path :{}",
			runtime_impl_path.to_str().unwrap_or("Invalid UTF-8 path")
		))
	})?;
	Ok(())
}

pub fn add_use_statements(file_path: &Path, use_statements: Vec<ItemUse>) -> Result<(), Error> {
	// Preserve the first use statement to insert the new one where they're
	let preserver = types::Preserver::new("use");
	let mut ast = preserver::preserve_and_parse(fs::read_to_string(file_path)?, vec![preserver])?;

	use_statements.into_iter().for_each(|use_statement| {
		if !parse::find_use_statement(&ast, &use_statement) {
			expand::expand_add_use_statement(&mut ast, use_statement)
		}
	});

	let generated_code = preserver::resolve_preserved(unparse(&ast));

	fs::write(file_path, &generated_code).map_err(|_| {
		Error::WriteError(format!("Path :{}", file_path.to_str().unwrap_or("Invalid UTF-8 path")))
	})?;

	Ok(())
}

pub fn add_composite_enums(file_path: &Path, composite_enums: Vec<ItemEnum>) -> Result<(), Error> {
	let mut preserver = types::Preserver::new("pub mod pallet");
	preserver.add_inners(vec!["pub struct Pallet"]);
	let mut ast = preserver::preserve_and_parse(fs::read_to_string(file_path)?, vec![preserver])?;

	composite_enums.into_iter().for_each(|composite_enum| {
		if !parse::find_composite_enum(&ast, &composite_enum) {
			expand::expand_pallet_add_composite_enum(&mut ast, composite_enum);
		}
	});

	let generated_code = preserver::resolve_preserved(unparse(&ast));

	fs::write(file_path, &generated_code).map_err(|_| {
		Error::WriteError(format!("Path :{}", file_path.to_str().unwrap_or("Invalid UTF-8 path")))
	})?;

	Ok(())
}
