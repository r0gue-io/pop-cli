// SPDX-License-Identifier: GPL-3.0

use super::*;
use std::{fs, path::PathBuf};
use syn::{parse_file, parse_quote, File};

struct TestBuilder {
	test_files: PathBuf,
	pub(crate) ast: File,
}

impl Default for TestBuilder {
	fn default() -> Self {
		Self {
			test_files: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
				.join("src")
				.join("rust_writer_helpers")
				.join("sample_files"),
			ast: parse_quote! {},
		}
	}
}

macro_rules! add_ast_to_builder_no_preserve{
    ($([$name: ident, $file: literal]),*) => {
        $(
            fn $name(&mut self){
            self.ast = parse_file(
                &fs::read_to_string(self.test_files.join($file))
                    .expect(concat!{"Error reading file in ", stringify!($name)}),
                )
                .expect(concat!{"Error parsing file in ", stringify!($name)});
            }
        )*
    };
}

impl TestBuilder {
	add_ast_to_builder_no_preserve! {
		[add_basic_pallet_ast, "basic_pallet.rs"],
		[add_runtime_using_runtime_macro_ast, "runtime_using_runtime_macro.rs"],
		[add_runtime_using_construct_runtime_macro_ast, "runtime_using_construct_runtime_macro.rs"]
	}
}

#[test]
fn find_highest_pallet_index_works_well() {
	let mut test_builder = TestBuilder::default();

	//find_highest_pallet_index should work with a runtime using the #[runtime] macro
	test_builder.add_runtime_using_runtime_macro_ast();

	let highest_index = find_highest_pallet_index(&test_builder.ast)
		.expect("find_highest_pallet_index is supposed to be Ok");

	// The highest index in the sample pallet is 11
	assert_eq!(highest_index.to_string(), "12");
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
