// SPDX-License-Identifier: GPL-3.0

use super::*;
use crate::rust_writer::helpers;
use std::{fs, path::PathBuf};
use syn::parse_str;

struct TestBuilder {
	test_files: PathBuf,
	ast: File,
}

impl Default for TestBuilder {
	fn default() -> Self {
		Self {
			test_files: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
				.join("src")
				.join("rust_writer")
				.join("test_files"),
			ast: parse_quote! {},
		}
	}
}

macro_rules! add_ast_to_builder{
    ($([$name: ident, $file: literal $(, $macro_excluded: literal)?]),*) => {
        $(
            fn $name(&mut self){
            self.ast = helpers::preserve_and_parse(
                fs::read_to_string(self.test_files.join($file))
                    .expect(concat!{"Error reading file in ", stringify!($name)}),
                vec![$($macro_excluded)?])
                .expect(concat!{"Error parsing file in ", stringify!($name)});
            }
        )*
    };
}

impl TestBuilder {
	add_ast_to_builder! {
		[add_basic_pallet_ast, "basic_pallet.rs"],
		[add_basic_pallet_with_config_preludes_ast, "basic_pallet_with_config_preludes.rs"],
		[add_outer_config_preludes_ast, "outer_config_preludes.rs"],
		[add_runtime_using_runtime_macro_ast, "runtime_using_runtime_macro.rs"],
		[add_runtime_using_construct_runtime_macro_ast, "runtime_using_construct_runtime_macro.rs", "construct_runtime"]
	}

	fn assert_item_in_config_trait(&self, contains: bool, checked_item: TraitItem) {
		let mut assert_happened = false;
		for item in &self.ast.items {
			match item {
				Item::Mod(ItemMod { ident, content, .. })
					if *ident == "pallet" && content.is_some() =>
				{
					let (_, items) =
						content.as_ref().expect("content is always Some thanks to the match guard");
					for item in items {
						match item {
							Item::Trait(ItemTrait { ident, items, .. }) if *ident == "Config" => {
								assert_eq!(items.contains(&checked_item), contains);
								assert_happened = true;
							},
							_ => continue,
						}
					}
				},
				_ => continue,
			}
		}
		assert!(assert_happened);
	}

	fn assert_type_added_to_config_preludes(&self, contains: bool, type_: ImplItem) {
		let mut assert_happened = false;
		for item in &self.ast.items {
			match item {
				// In case ast using inner config_preludes
				Item::Mod(ItemMod { ident, content, .. })
					if *ident == "pallet" && content.is_some() =>
				{
					let (_, items) =
						content.as_ref().expect("content is always Some thanks to the match guard");
					for item in items {
						match item {
							Item::Mod(ItemMod { ident, content, .. })
								if *ident == "config_preludes" && content.is_some() =>
							{
								let (_, items) = content
									.as_ref()
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
										{
											assert_eq!(items.contains(&type_), contains);
											assert_happened = true;
										},
										_ => continue,
									}
								}
							},
							_ => continue,
						}
					}
				},
				// In case ast using an outer config preludes
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
				{
					assert_eq!(items.contains(&type_), contains);
					assert_happened = true;
				},
				_ => continue,
			}
		}
		assert!(assert_happened);
	}

	fn assert_pallet_in_runtime(
		&self,
		contains: bool,
		expected_index: Literal,
		used_macro: RuntimeUsedMacro,
		pallet_name: Type,
		pallet_item: Ident,
	) {
		let mut assert_happened = false;
		match used_macro {
			RuntimeUsedMacro::Runtime =>
				for item in &self.ast.items {
					match item {
						Item::Mod(ItemMod { ident, content, .. })
							if *ident == "runtime" && content.is_some() =>
						{
							let (_, items) = content
								.as_ref()
								.expect("content is always Some thanks to the match guard");

							assert_eq!(
								items.contains(&Item::Type(parse_quote! {
									#[runtime::pallet_index(#expected_index)]
									pub type #pallet_item = #pallet_name;
								})),
								contains
							);
							assert_happened = true;
						},
						_ => continue,
					}
				},
			RuntimeUsedMacro::ConstructRuntime =>
				for item in &self.ast.items {
					match item {
						Item::Macro(ItemMacro {
							mac: Macro { path: syn::Path { segments, .. }, tokens, .. },
							..
						}) if segments
							.iter()
							.any(|segment| segment.ident == "construct_runtime") =>
						{
							let mut token_tree: Vec<TokenTree> =
								tokens.clone().into_iter().collect();
							for token in token_tree.iter_mut() {
								if let TokenTree::Group(group) = token {
									let new_pallet_token_stream: TokenStream = parse_quote! {
										#pallet_item:#pallet_name,
									};
									assert_eq!(
										group
											.stream()
											.to_string()
											.contains(&new_pallet_token_stream.to_string()),
										contains
									);
									assert_happened = true;
								}
							}
						},
						_ => continue,
					}
				},
			_ => (),
		}
		assert!(assert_happened);
	}

	fn assert_impl_block_contained(
		&self,
		contains: bool,
		pallet_name: Ident,
		parameter_types: Vec<ParameterTypes>,
		using_default_config: bool,
	) {
		if !parameter_types.is_empty() {
			let parameter_idents: Vec<&Ident> =
				parameter_types.iter().map(|item| &item.ident).collect();
			let parameter_types_types: Vec<&Type> =
				parameter_types.iter().map(|item| &item.type_).collect();
			let parameter_values: Vec<&Expr> =
				parameter_types.iter().map(|item| &item.value).collect();

			assert_eq!(
				self.ast.items.contains(&Item::Macro(parse_quote! {
					///EMPTY_LINE
					parameter_types!{
						#(
							pub #parameter_idents: #parameter_types_types = #parameter_values;
						)*
					}
				})),
				contains
			);
		}

		if using_default_config {
			assert_eq!(
				self.ast.items.contains(&Item::Impl(parse_quote! {
					///EMPTY_LINE
					#[derive_impl(#pallet_name::config_preludes::TestDefaultConfig)]
					impl #pallet_name::Config for Runtime{}
				})),
				contains
			);
		} else {
			assert_eq!(
				self.ast.items.contains(&Item::Impl(parse_quote! {
					///EMPTY_LINE
					impl #pallet_name::Config for Runtime{}
				})),
				contains
			);
		}
	}

	fn assert_type_in_impl_block(
		&self,
		contains: bool,
		type_name: Ident,
		runtime_value: Type,
		pallet_name: &str,
	) {
		let mut assert_happened = false;
		for item in &self.ast.items {
			match item {
				Item::Impl(ItemImpl {
					trait_: Some((_, syn::Path { segments, .. }, _)),
					items,
					..
				}) if segments.iter().any(|segment| segment.ident == pallet_name) => {
					assert_eq!(
						items.contains(&ImplItem::Type(parse_quote! {
							type #type_name = #runtime_value;
						})),
						contains
					);
					assert_happened = true;
				},
				_ => continue,
			}
		}
		assert!(assert_happened);
	}

	fn assert_use_statement_included(&self, contains: bool, use_statement: ItemUse) {
		assert_eq!(self.ast.items.contains(&Item::Use(use_statement)), contains);
	}
}

#[test]
fn expand_pallet_config_trait_works_well_test() {
	let mut test_builder = TestBuilder::default();
	//This test modifies the config trait of the pallet, so the ast contained in the builder is
	//the basic pallet's ast.
	test_builder.add_basic_pallet_ast();

	//A helper type to pass to expand_pallet_config_trait.
	let mut default_config_type =
		DefaultConfigType::Default { type_default_impl: parse_quote! {type whatever = ();} };

	//Check that the config trait doesn't include ```MyDefaultType```.
	test_builder.assert_item_in_config_trait(
		false,
		TraitItem::Type(parse_quote! {
			///EMPTY_LINE
			type MyDefaultType: Bound1 + From<Trait2> +;
		}),
	);

	//Expand the pallet config trait with our type.
	expand_pallet_config_trait(
		&mut test_builder.ast,
		&default_config_type,
		Ident::new("MyDefaultType", Span::call_site()),
		vec![parse_quote! {Bound1}, parse_quote! {From<Trait2>}],
	);

	//Now ```MyDefaultType``` is part of the ast.
	test_builder.assert_item_in_config_trait(
		true,
		TraitItem::Type(parse_quote! {
			///EMPTY_LINE
			type MyDefaultType: Bound1 + From<Trait2> +;
		}),
	);

	test_builder.assert_item_in_config_trait(
		false,
		TraitItem::Type(parse_quote! {
			///EMPTY_LINE
			#[pallet::no_default]
			type MyNoDefaultType: Bound1 + From<Trait2> +;
		}),
	);

	default_config_type = DefaultConfigType::NoDefault;
	expand_pallet_config_trait(
		&mut test_builder.ast,
		&default_config_type,
		Ident::new("MyNoDefaultType", Span::call_site()),
		vec![parse_quote! {Bound1}, parse_quote! {From<Trait2>}],
	);

	test_builder.assert_item_in_config_trait(
		true,
		TraitItem::Type(parse_quote! {
			///EMPTY_LINE
			#[pallet::no_default]
			type MyNoDefaultType: Bound1 + From<Trait2> +;
		}),
	);

	test_builder.assert_item_in_config_trait(
		false,
		TraitItem::Type(parse_quote! {
			///EMPTY_LINE
			#[pallet::no_default_bounds]
			type MyNoDefaultBoundsType: Bound1 + From<Trait2> +;
		}),
	);

	default_config_type = DefaultConfigType::NoDefaultBounds {
		type_default_impl: parse_quote! {type whatever = ();},
	};
	expand_pallet_config_trait(
		&mut test_builder.ast,
		&default_config_type,
		Ident::new("MyNoDefaultBoundsType", Span::call_site()),
		vec![parse_quote! {Bound1}, parse_quote! {From<Trait2>}],
	);

	test_builder.assert_item_in_config_trait(
		true,
		TraitItem::Type(parse_quote! {
			///EMPTY_LINE
			#[pallet::no_default_bounds]
			type MyNoDefaultBoundsType: Bound1 + From<Trait2> +;
		}),
	);
}

#[test]
fn expand_pallet_config_preludes_inner_module_works_well_test() {
	let mut test_builder = TestBuilder::default();
	//This test uses a pallet lib containing config_preludes.
	test_builder.add_basic_pallet_with_config_preludes_ast();

	// Type to add
	let my_type = ImplItem::Type(parse_quote! {
		type MyType = ();
	});

	//Check that the config type's not included.
	test_builder.assert_type_added_to_config_preludes(false, my_type.clone());

	//Expand the pallet's config_preludes.
	expand_pallet_config_preludes(&mut test_builder.ast, my_type.clone());

	//Check that the config type's included.
	test_builder.assert_type_added_to_config_preludes(true, my_type.clone());
}

#[test]
fn expand_pallet_config_preludes_outer_file_works_well_test() {
	let mut test_builder = TestBuilder::default();
	//This test uses a pallet lib containing config_preludes.
	test_builder.add_outer_config_preludes_ast();

	// Type to add
	let my_type = ImplItem::Type(parse_quote! {
		type MyType = ();
	});

	//Check that the config type's not included.
	test_builder.assert_type_added_to_config_preludes(false, my_type.clone());

	//Expand the pallet's config_preludes.
	expand_pallet_config_preludes(&mut test_builder.ast, my_type.clone());

	//Check that the config type's included.
	test_builder.assert_type_added_to_config_preludes(true, my_type.clone());
}

#[test]
fn expand_runtime_add_pallet_using_runtime_macro_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_runtime_using_runtime_macro_ast();

	// Arbitrary highest index to pass to expand_runtime_add_pallet
	let highest_index = 11u8;
	// Expected index as syn::literal
	let expected_index = Literal::u8_unsuffixed(highest_index.saturating_add(1));

	let pallet_item = Ident::new("Test", Span::call_site());
	let pallet_name: Type = parse_str("pallet_test").expect(
		"Error parsing pallet_test in add_pallet_to_runtime_using_runtime_macro_works_well_test",
	);

	test_builder.assert_pallet_in_runtime(
		false,
		expected_index.clone(),
		RuntimeUsedMacro::Runtime,
		pallet_name.clone(),
		pallet_item.clone(),
	);

	expand_runtime_add_pallet(
		&mut test_builder.ast,
		highest_index,
		RuntimeUsedMacro::Runtime,
		pallet_name.clone(),
		pallet_item.clone(),
	);

	test_builder.assert_pallet_in_runtime(
		true,
		expected_index,
		RuntimeUsedMacro::Runtime,
		pallet_name,
		pallet_item,
	);
}

#[test]
fn expand_runtime_add_pallet_using_construct_runtime_macro_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_runtime_using_construct_runtime_macro_ast();

	// Expected index as syn::literal, needed for assert_pallet_in_runtime but not relevant in this
	// case
	let expected_index = Literal::u8_unsuffixed(0u8);

	let pallet_item = Ident::new("Test", Span::call_site());
	let pallet_name: Type = parse_str("pallet_test").expect(
		"Error parsing pallet_test in add_pallet_to_runtime_using_runtime_macro_works_well_test",
	);

	test_builder.assert_pallet_in_runtime(
		false,
		expected_index.clone(),
		RuntimeUsedMacro::ConstructRuntime,
		pallet_name.clone(),
		pallet_item.clone(),
	);

	expand_runtime_add_pallet(
		&mut test_builder.ast,
		0u8,
		RuntimeUsedMacro::ConstructRuntime,
		pallet_name.clone(),
		pallet_item.clone(),
	);

	test_builder.assert_pallet_in_runtime(
		true,
		expected_index,
		RuntimeUsedMacro::ConstructRuntime,
		pallet_name,
		pallet_item,
	);
}

#[test]
fn expand_runtime_add_impl_block_without_default_config_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_runtime_using_runtime_macro_ast();

	let parameter_types = Vec::new();
	let pallet_name = Ident::new("Test", Span::call_site());

	// Impl pallet without defautl config not added
	test_builder.assert_impl_block_contained(
		false,
		pallet_name.clone(),
		parameter_types.clone(),
		false,
	);

	// Add it
	expand_runtime_add_impl_block(
		&mut test_builder.ast,
		pallet_name.clone(),
		parameter_types.clone(),
		false,
	);

	// Impl pallet without default config added.
	test_builder.assert_impl_block_contained(
		true,
		pallet_name.clone(),
		parameter_types.clone(),
		false,
	);
}

#[test]
fn expand_runtime_add_impl_block_with_default_config_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_runtime_using_runtime_macro_ast();

	let pallet_name = Ident::new("Test", Span::call_site());
	let parameter_types = Vec::new();

	// Impl pallet with default config not added
	test_builder.assert_impl_block_contained(
		false,
		pallet_name.clone(),
		parameter_types.clone(),
		true,
	);

	// Add it
	expand_runtime_add_impl_block(
		&mut test_builder.ast,
		pallet_name.clone(),
		parameter_types.clone(),
		true,
	);

	//Impl pallet with default config added
	test_builder.assert_impl_block_contained(
		true,
		pallet_name.clone(),
		parameter_types.clone(),
		true,
	);
}

#[test]
fn expand_runtime_add_impl_block_using_parameter_types_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_runtime_using_runtime_macro_ast();

	let pallet_name = Ident::new("Test", Span::call_site());
	let parameter_types = vec![
		ParameterTypes {
			ident: Ident::new("MyType1", Span::call_site()),
			type_: parse_quote! {Type},
			value: parse_quote! {Default::default()},
		},
		ParameterTypes {
			ident: Ident::new("MyType2", Span::call_site()),
			type_: parse_quote! {Type},
			value: parse_quote! {Default::default()},
		},
	];

	// Impl pallet block + parameter_types block not added
	test_builder.assert_impl_block_contained(
		false,
		pallet_name.clone(),
		parameter_types.clone(),
		true,
	);

	// Add them
	expand_runtime_add_impl_block(
		&mut test_builder.ast,
		pallet_name.clone(),
		parameter_types.clone(),
		true,
	);

	//Impl pallet block + parameter_types block not added
	test_builder.assert_impl_block_contained(
		true,
		pallet_name.clone(),
		parameter_types.clone(),
		true,
	);
}

#[test]
fn expand_runtime_add_type_to_impl_block_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_runtime_using_runtime_macro_ast();

	let pallet_name = "Test";
	let type_name = Ident::new("MyType", Span::call_site());
	let runtime_value: Type = parse_quote! {Type};
	let parameter_types = Vec::new();

	expand_runtime_add_impl_block(
		&mut test_builder.ast,
		Ident::new(pallet_name, Span::call_site()),
		parameter_types,
		false,
	);

	// The pallet impl block doesn't include the type
	test_builder.assert_type_in_impl_block(
		false,
		type_name.clone(),
		runtime_value.clone(),
		pallet_name,
	);

	// Add it
	expand_runtime_add_type_to_impl_block(
		&mut test_builder.ast,
		type_name.clone(),
		runtime_value.clone(),
		pallet_name,
	);

	// Now it's included in the ast
	test_builder.assert_type_in_impl_block(
		true,
		type_name.clone(),
		runtime_value.clone(),
		pallet_name,
	);
}

#[test]
fn expand_add_use_statement_works_well_test() {
	let mut test_builder = TestBuilder::default();
	test_builder.add_basic_pallet_ast();

	let use_statement: ItemUse = parse_quote! {
		use some_crate::some_module::some_function;
	};

	test_builder.assert_use_statement_included(false, use_statement.clone());

	expand_add_use_statement(&mut test_builder.ast, use_statement.clone());

	test_builder.assert_use_statement_included(true, use_statement);
}
