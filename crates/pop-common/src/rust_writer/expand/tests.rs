// SPDX-License-Identifier: GPL-3.0

use super::*;
use crate::rust_writer::helpers;
use std::{fs, path::PathBuf};

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
								if contains {
									assert!(items.contains(&checked_item));
								} else {
									assert!(!items.contains(&checked_item));
								}
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
											if contains {
												assert!(items.contains(&type_));
											} else {
												assert!(!items.contains(&type_));
											}
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
					if contains {
						assert!(items.contains(&type_));
					} else {
						assert!(!items.contains(&type_));
					}
					assert_happened = true;
				},
				_ => continue,
			}
		}
		assert!(assert_happened);
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
