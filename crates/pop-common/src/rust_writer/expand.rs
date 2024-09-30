// SPDX-License-Identifier: GPL-3.0

use crate::rust_writer::{helpers::capitalize_str, types::*};
use proc_macro2::{Group, Literal, Span, TokenStream, TokenTree};
use syn::{
	parse_quote, Expr, File, Ident, ImplItem, Item, ItemImpl, ItemMacro, ItemMod, ItemTrait, Macro,
	Meta, MetaList, TraitItem, Type,
};

pub(crate) fn expand_pallet_config_trait(
	ast: &mut File,
	default_config: DefaultConfigType,
	type_name: &str,
	trait_bounds: Vec<&str>,
) {
	let type_name = Ident::new(&capitalize_str(type_name), Span::call_site());
	let trait_bounds = trait_bounds
		.iter()
		.map(|bound| Ident::new(&capitalize_str(bound), Span::call_site()))
		.collect::<Vec<Ident>>();
	for item in &mut ast.items {
		match item {
			Item::Mod(ItemMod { ident, content, .. })
				if *ident == "pallet" && content.is_some() =>
			{
				let (_, items) =
					content.as_mut().expect("content is always Some thanks to the match guard");
				for item in items {
					match item {
						Item::Trait(ItemTrait { ident, items, .. }) if *ident == "Config" => {
							items.push(match default_config {
								DefaultConfigType::Default => TraitItem::Type(parse_quote! {
									///EMPTY_LINE
									type #type_name: #(#trait_bounds +)*;
								}),
								DefaultConfigType::NoDefault => TraitItem::Type(parse_quote! {
									///EMPTY_LINE
									#[pallet::no_default]
									type #type_name: #(#trait_bounds +)*;
								}),
								DefaultConfigType::NoDefaultBounds => {
									TraitItem::Type(parse_quote! {
										///EMPTY_LINE
										#[pallet::no_default_bounds]
										type #type_name: #(#trait_bounds +)*;
									})
								},
							});
						},
						_ => continue,
					}
				}
			},
			_ => continue,
		}
	}
}

pub(crate) fn expand_pallet_config_preludes(ast: &mut File, type_name: &str, default_value: &str) {
	let type_name = Ident::new(&capitalize_str(type_name), Span::call_site());
	let default_value = Ident::new(default_value, Span::call_site());

	for item in &mut ast.items {
		match item {
			// In case file_path points to lib.rs, config_preludes is contained inside pallet mod in
			// lib.rs so we have to look for that module and the impl blocks for structs defined
			// inside it, equivalently impl blocks using the register_default_impl macro
			Item::Mod(ItemMod { ident, content, .. })
				if *ident == "pallet" && content.is_some() =>
			{
				let (_, items) =
					content.as_mut().expect("content is always Some thanks to the match guard");
				for item in items {
					match item {
						Item::Mod(ItemMod { ident, content, .. })
							if *ident == "config_preludes" && content.is_some() =>
						{
							let (_, items) = content
								.as_mut()
								.expect("content is always Some thanks to the match guard");
							for item in items {
								match item {
									Item::Impl(ItemImpl { attrs, items, .. })
										if attrs.iter().any(|attribute| {
											if let Meta::List(MetaList {
												path: syn::Path { segments, .. },
												..
											}) = &attribute.meta
											{
												segments.iter().any(|segment| {
													segment.ident == "register_default_impl"
												})
											} else {
												false
											}
										}) =>
										items.push(ImplItem::Type(parse_quote! {
											type #type_name = #default_value;
										})),
									_ => continue,
								}
							}
						},
						_ => continue,
					}
				}
			},
			// In case file_path points to config_preludes.rs, we have to look for the impl blocks
			// for structs defined inside the file.
			Item::Impl(ItemImpl { attrs, items, .. })
				if attrs.iter().any(|attribute| {
					if let Meta::List(MetaList { path: syn::Path { segments, .. }, .. }) =
						&attribute.meta
					{
						segments.iter().any(|segment| segment.ident == "register_default_impl")
					} else {
						false
					}
				}) =>
				items.push(ImplItem::Type(parse_quote! {
					type #type_name = #default_value;
				})),
			_ => continue,
		}
	}
}

pub(crate) fn expand_runtime_add_pallet(
	ast: &mut File,
	highest_index: u8,
	used_macro: RuntimeUsedMacro,
	pallet_name: &str,
	pallet_item: &str,
) {
	let pallet_item = Ident::new(pallet_item, Span::call_site());
	let pallet_name = Ident::new(pallet_name, Span::call_site());

	match used_macro {
		RuntimeUsedMacro::Runtime => {
			// Convert the highest_index in Ident
			let highest_index = Literal::u8_unsuffixed(highest_index.saturating_add(1));
			for item in &mut ast.items {
				match item {
					Item::Mod(ItemMod { ident, content, .. })
						if *ident == "runtime" && content.is_some() =>
					{
						let (_, items) = content
							.as_mut()
							.expect("content is always Some thanks to the match guard");
						items.push(Item::Type(parse_quote! {
							#[runtime::pallet_index(#highest_index)]
							pub type #pallet_item = #pallet_name;
						}));
					},
					_ => continue,
				}
			}
		},
		RuntimeUsedMacro::ConstructRuntime => {
			for item in &mut ast.items {
				match item {
					Item::Macro(ItemMacro {
						mac: Macro { path: syn::Path { segments, .. }, tokens, .. },
						..
					}) if segments.iter().any(|segment| segment.ident == "construct_runtime") => {
						// construct_runtime! contains the definition of either a struct or an
						// enum whose variants/items are the pallets, so basically the pallets
						// are containedd inside a TokenTree::Group and we can suppose that
						// there's nothing else inside construct_runtime, so it's enough with
						// finding a group.
						let mut token_tree: Vec<TokenTree> = tokens.clone().into_iter().collect();
						for token in token_tree.iter_mut() {
							if let TokenTree::Group(group) = token {
								let mut stream = group.stream();
								stream.extend::<TokenStream>(parse_quote! {
									#pallet_item: #pallet_name,
								});
								let new_group = Group::new(group.delimiter(), stream);
								*token = TokenTree::Group(new_group);
							}
						}
						*tokens = TokenStream::from_iter(token_tree);
					},
					_ => continue,
				}
			}
		},
		RuntimeUsedMacro::NotFound => (),
	}
}

pub(crate) fn expand_runtime_add_impl_block(
	ast: &mut File,
	pallet_name: &str,
	parameter_types: Vec<ParameterTypes>,
	types: Vec<String>,
	values: Vec<Type>,
	default_config: bool,
) {
	let pallet_name = Ident::new(pallet_name, Span::call_site());
	let types: Vec<Ident> = types
		.iter()
		.map(|type_| Ident::new(&capitalize_str(type_), Span::call_site()))
		.collect();
	let parameter_idents: Vec<Ident> = parameter_types
		.iter()
		.map(|item| Ident::new(&capitalize_str(&item.ident), Span::call_site()))
		.collect();
	let parameter_types_types: Vec<&Type> =
		parameter_types.iter().map(|item| &item.type_).collect();
	let parameter_values: Vec<&Expr> = parameter_types.iter().map(|item| &item.value).collect();
	let items = &mut ast.items;

	// It's enough checking that parameter_idents isn't empty, by construction all the 3 Vec have
	// the same lenght
	if !parameter_idents.is_empty() {
		items.push(Item::Macro(parse_quote! {
			///EMPTY_LINE
			parameter_types!{
				#(
					pub #parameter_idents: #parameter_types_types = #parameter_values;
				)*
			}
		}));
	}
	if default_config {
		items.push(Item::Impl(parse_quote! {
			///EMPTY_LINE
			#[derive_impl(#pallet_name::config_preludes::TestDefaultConfig)]
			impl #pallet_name::Config for Runtime{
				#(
					type #types = #values;
				)*
			}
		}));
	} else {
		items.push(Item::Impl(parse_quote! {
			///EMPTY_LINE
			impl #pallet_name::Config for Runtime{
				#(
					type #types = #values;
				)*
			}
		}));
	}
}

pub(crate) fn expand_runtime_add_type_to_impl_block(
	ast: &mut File,
	type_name: &str,
	runtime_value: &str,
	pallet_name: &str,
) {
	let type_name = Ident::new(&capitalize_str(type_name), Span::call_site());
	let runtime_value = Ident::new(runtime_value, Span::call_site());
	for item in &mut ast.items {
		match item {
			Item::Impl(ItemImpl {
				trait_: Some((_, syn::Path { segments, .. }, _)),
				items,
				..
			}) if segments.iter().any(|segment| segment.ident == pallet_name) =>
				items.push(ImplItem::Type(parse_quote! {
					type #type_name = #runtime_value;
				})),
			_ => continue,
		}
	}
}
