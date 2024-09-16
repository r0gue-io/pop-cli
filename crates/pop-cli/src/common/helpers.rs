// SPDX-License-Identifier: GPL-3.0

/// A macro to facilitate the select multiple variant of an enum and store them inside a `Vec`.
/// # Arguments
/// * `$enum`: The enum type to be iterated over for the selection. This enum must implement
///   `IntoEnumIterator` and `EnumMessage` traits from the `strum` crate. Each variant is
///   responsible of its own messages.
/// * `$prompt_message`: The message displayed to the user. It must implement the `Display` trait.
/// * `$excluded_variants`: If the enum contain variants that shouldn't be included in the
///   multiselect pick, they're specified here. This is useful if a enum is used in a few places and
///   not all of them need all the variants but share some of them. It has to be a `Vec`;
/// # Note
/// This macro only works with a 1-byte sized enums, this is, fieldless enums with at most 255
/// elements each. This is because we're just interested in letting the user to pick some options
/// among a predefined set, then the name should be descriptive enough, and 1-byte sized enums are
/// really easy to convert to and from a `u8`, so we can work with `u8` all the time and just
/// recover the variant at the end.
///
/// The decision of using 1-byte enums instead of just fieldless enums is for simplicity: we won't
/// probably offer a user to pick from > 256 options. If this macro is used with enums containing
/// fields, the conversion to `u8` will simply be detected at compile time and the compilation will
/// fail. If this macro is used with fieldless enums greater than 1-byte (really weird but
/// possible), the conversion to u8 will overflow and lead to unexpected behavior, so we panic at
/// runtime if that happens for completeness.
///
/// # Example
///
/// ```rust
/// use strum::{IntoEnumIterator, EnumMessage};
/// use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};
/// use cliclack::{multiselect};
/// use pop_common::multiselect_pick;
///
/// #[derive(Debug, EnumIter, EnumMessageDerive, Copy, Clone)]
/// enum FieldlessEnum {
///     #[strum(message = "Type 1", detailed_message = "Detailed message for Type 1")]
///     Type1,
///     #[strum(message = "Type 2", detailed_message = "Detailed message for Type 2")]
///     Type2,
///     #[strum(message = "Type 3", detailed_message = "Detailed message for Type 3")]
///     Type3,
/// }
///
/// fn test_function() -> Result<(),std::io::Error>{
///     let vec = multiselect_pick!(FieldlessEnum, "Hello, world!");
///     Ok(())
/// }
/// ```
///
/// # Requirements
///
/// This macro requires the following imports to function correctly:
///
/// ```rust
/// use cliclack::{multiselect};
/// use strum::{EnumMessage, IntoEnumIterator};
/// ```
///
/// Additionally, this macro handle results, so it must be used inside a function doing so.
/// Otherwise the compilation will fail.
#[macro_export]
macro_rules! multiselect_pick {
	($enum: ty, $prompt_message: expr $(, $excluded_variants: expr)?) => {{
        // Ensure the enum is 1-byte long. This is needed cause fieldless enums with > 256 elements
		// will lead to unexpected behavior as the conversion to u8 for them isn't detected as wrong
		// at compile time. Enums containing variants with fields will be catched at compile time.
		// Weird but possible.
		assert_eq!(std::mem::size_of::<$enum>(), 1);
		let mut prompt = multiselect(format!(
			"{} {}",
			$prompt_message,
			"Pick an option by pressing the spacebar. Press enter when you're done!"
		))
		.required(false);

		for variant in <$enum>::iter() {
            $(if $excluded_variants.contains(&variant){continue; })?
			prompt = prompt.item(
				variant as u8,
				variant.get_message().unwrap_or_default(),
				variant.get_detailed_message().unwrap_or_default(),
			);
		}

		// The unsafe block is safe cause the bytes are the discriminants of the enum picked above,
		// qed;
		prompt
			.interact()?
			.iter()
			.map(|byte| unsafe { std::mem::transmute(*byte) })
			.collect::<Vec<$enum>>()
	}};
}
