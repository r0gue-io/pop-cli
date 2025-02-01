// SPDX-License-Identifier: GPL-3.0

use std::{fs, path::PathBuf};
use syn::{parse_file, parse_quote, File};

pub(crate) struct TestBuilder {
	test_files: PathBuf,
	pub(crate) ast: File,
}

impl Default for TestBuilder {
	fn default() -> Self {
		Self {
			test_files: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
				.join("src")
				.join("rust_writer")
				.join("tests")
				.join("sample_files"),
			ast: parse_quote! {},
		}
	}
}

macro_rules! add_ast_to_builder_no_preserve{
    ($([$name: ident, $file: literal]),*) => {
        $(
            pub(crate) fn $name(&mut self){
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
		[add_basic_pallet_with_config_preludes_ast, "basic_pallet_with_config_preludes.rs"],
		[add_basic_pallet_with_composite_enum_ast, "basic_pallet_with_composite_enum.rs"],
		[add_outer_config_preludes_ast, "outer_config_preludes.rs"],
		[add_runtime_using_runtime_macro_ast, "runtime_using_runtime_macro.rs"],
		[add_runtime_using_construct_runtime_macro_ast, "runtime_using_construct_runtime_macro.rs"]
	}
}
