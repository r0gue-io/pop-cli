// SPDX-License-Identifier: GPL-3.0

use super::*;
use crate::rust_writer::tests::test_builder::TestBuilder;
use syn::parse_quote;

#[test]
fn find_highest_pallet_index_works_well() {
	let mut test_builder = TestBuilder::default();

	//find_highest_pallet_index should work with a runtime using the #[runtime] macro
	test_builder.add_runtime_using_runtime_macro_ast();

	let highest_index = find_highest_pallet_index(&test_builder.ast)
		.expect("find_highest_pallet_index is supposed to be Ok");

	// The highest index in the sample pallet is 11
	assert_eq!(highest_index, 11);
}

#[test]
fn find_highest_pallet_index_fails_if_input_doesnt_use_runtime_macro() {
	let mut test_builder = TestBuilder::default();

	//Add a pallet file to the test_builder
	test_builder.add_runtime_using_construct_runtime_macro_ast();

	let failed_call = find_highest_pallet_index(&test_builder.ast);

	assert!(failed_call.is_err());
	if let Error::Descriptive(msg) = failed_call.unwrap_err() {
		assert_eq!(msg, "Unable to find the highest pallet index in runtime file");
	} else {
		panic!("find_highest_pallet_index should return only Error::Descriptive")
	}
}

#[test]
fn find_used_runtime_macro_with_construct_runtime_works_well() {
	let mut test_builder = TestBuilder::default();

	//Add the runtime with construct_runtime to the test_builder
	test_builder.add_runtime_using_construct_runtime_macro_ast();

	let used_macro = find_used_runtime_macro(&test_builder.ast)
		.expect("find_used_runtime_macro is supposed to be Ok");

	assert_eq!(used_macro, RuntimeUsedMacro::ConstructRuntime);
}

#[test]
fn find_used_runtime_macro_with_runtime_macro_works_well() {
	let mut test_builder = TestBuilder::default();

	//Add the runtime with runtime to the test_builder
	test_builder.add_runtime_using_runtime_macro_ast();

	let used_macro = find_used_runtime_macro(&test_builder.ast)
		.expect("find_used_runtime_macro is supposed to be Ok");

	assert_eq!(used_macro, RuntimeUsedMacro::Runtime);
}

#[test]
fn find_used_runtime_macro_fails_if_input_isnt_runtime_file() {
	let mut test_builder = TestBuilder::default();

	//Add a pallet file to the test_builder
	test_builder.add_basic_pallet_ast();

	let failed_call = find_used_runtime_macro(&test_builder.ast);

	assert!(failed_call.is_err());
	if let Error::Descriptive(msg) = failed_call.unwrap_err() {
		assert_eq!(msg, "Unable to find a runtime declaration in runtime file");
	} else {
		panic!("find_used_runtime_macro should return only Error::Descriptive")
	}
}

#[test]
fn find_use_statement_works_well() {
	let mut test_builder = TestBuilder::default();

	test_builder.add_basic_pallet_ast();

	//Basic pallet has the use statement pub use pallet::*;
	let valid_use_statement: ItemUse = parse_quote! {pub use pallet::*;};
	let invalid_use_statement: ItemUse = parse_quote! {use std::path::Path;};

	assert!(find_use_statement(&test_builder.ast, &valid_use_statement));
	assert!(!find_use_statement(&test_builder.ast, &invalid_use_statement));
}

#[test]
fn find_composite_enum_works_well() {
	let mut test_builder = TestBuilder::default();

	test_builder.add_basic_pallet_with_composite_enum_ast();

	// This enum appears in the sample file basic_pallet_with_composite_enum
	let composite_enum: ItemEnum = parse_quote! {
			#[pallet::composite_enum]
			pub enum SomeEnum {
				#[codec(index = 0)]
				Something,
			}
	};

	// This enum doesn't appear in the sample file basic_pallet_with_composite_enum
	let bad_composite_enum: ItemEnum = parse_quote! {
		#[pallet::composite_enum]
		pub enum OtherEnum{
			#[codec(index=0)]
			Something,
		}
	};

	assert!(find_composite_enum(&test_builder.ast, &composite_enum));
	assert!(!find_composite_enum(&test_builder.ast, &bad_composite_enum));

	// basic pallet contains an enum called SomeEnum but it's not annotated as composite_enum
	let mut test_builder = TestBuilder::default();

	test_builder.add_basic_pallet_ast();
	assert!(!find_composite_enum(&test_builder.ast, &composite_enum));
}
