use cliclack::input;

/// A macro to facilitate the selection of an options among some pre-selected ones (represented as
/// variants from a fieldless enums) and the naming of that selection. This process is repetead
/// until the user stops it and then the result is stored in a `Vec` of tuples, containing the
/// options and the given names, which is returned at the end of the macro expansion. This is useful
/// in processes such as designing a pallet's storage to generate a pallet template: we need the
/// user to pick among the different storage types (StorageValue, StorageMap, StorageDoubleMap,...)
/// and give names to those storages.
///
/// # Parameters
///
/// - `$enum`: The enums types to be iterated over for selection. This enums must implement
///   `IntoEnumIterator` and `EnumMessage` traits from the `strum` crate. Each variant is
///   responsible of its own messages. At least one enum type has to be passed in, but several can
///   be used if for non-conflicting choices may be selected by the user for the same thing.
/// - `$prompt_message`: The messages displayed to the user at the very beginning of each iteration.
///   They must implement the `Dispay` trait. Each enum must have associated a prompt message.
/// # Note
/// This macro only works with a 1-byte sized enums, this is, fieldless enums with at most 255
/// elements each. This is because we're just interested in letting the user to pick among a
/// pre-defined set of options and then naming that selection, compounding that selection into a
/// tuple. Hence we have a tuple of all the selected items + the given name.
///
/// The decision of using fieldless enums leading to a `Vec` of tuples instead of using composite
/// enums and just using a `Vec` of them is based in the following points:
/// 1. The size of composite enums would be considerably greater than the ones used, which are just
///    1-byte each!
/// 2. This macro will be likely used to determine the options chosen by the user and to write a
///    template according to that selection. To pass that information together with the name to the
///    `rs.templ` file, we would need to parse it before, so using a `Vec` of tuples bypass this
///    step :)
/// 3. Using fieldless enums, identify each variant with bytes is straightforward, so we can play
///    with `u8` all the time and just recover the variant at the end. This allows us to add the
///    option of quitting the loop as a number, instead of adding it to all the enums using this
///    macro.
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
/// use strum_macros::{EnumIter, EnumMessage};
/// use strum::{IntoEnumIterator, EnumMessage};
/// use cliclack::{input, select};
///
/// #[derive(Debug, EnumIter, EnumMessage, Copy, Clone)]
/// enum FieldlessEnum {
///     #[strum(message = "Type 1", detailed_message = "Detailed message for Type 1")]
///     Type1,
///     #[strum(message = "Type 2", detailed_message = "Detailed message for Type 2")]
///     Type2,
///     #[strum(message = "Type 3", detailed_message = "Detailed message for Type 3")]
///     Type3,
/// }
///
/// #[derive(Debug, EnumIter, EnumMessage, Copy, Clone)]
/// enum FieldlessEnum2 {
///     #[strum(message = "Type 4", detailed_message = "Detailed message for Type 4")]
///     Type4,
/// }
///
/// let vec_1 = pick_options_and_give_name!((FieldlessEnum, "Hello world!"));
///
/// let vec_2 = pick_options_and_give_name!((FieldlessEnum, "Hello world!"), (FieldlessEnum2, "I'm a macro!"));
/// ```
///
/// # Requirements
///
/// This macro requires the following imports to function correctly:
///
/// ```rust
/// use cliclack::{input, select};
/// use strum::{EnumMessage, IntoEnumIterator};
/// ```
///
/// Additionally, this macro handle results, so it must be used inside a function doing so.
/// Otherwise the compilation will fail.
#[macro_export]
macro_rules! pick_options_and_give_name{
    ($(($enum: ty, $prompt_message: expr)),+) => {
        {
            let mut output = Vec::new();
            // Ensure the enums are 1-byte long. This is needed cause fieldless enums with > 256 elements will lead to unexpected behavior as the conversion to u8 for them isn't detected as wrong at compile time. Weird but possible
            $(
                assert!(std::mem::size_of::<$enum>() == 1);
            )+
            let mut enum_lens = Vec::new();
            $(
                enum_lens.push(<$enum>::iter().count());
            )+
            // Ensure that there's at least a free spot for quitting the loop. Weird but possible.
            enum_lens.iter().for_each(|len| assert!(*len < 255));

            loop{
                let mut index = 0;
                let mut selected_options = Vec::new();
                $(
                    index += 1;
                    let mut prompt = select($prompt_message).initial_value(0u8);
                    for variant in <$enum>::iter(){
                        prompt = prompt.item(
                            variant as u8,
                            variant.get_message().unwrap_or_default(),
                            variant.get_detailed_message().unwrap_or_default(),
                        );
                    };

                    prompt = prompt.item(
                        // This conversion is safe as we've ensured the lengths are smaller than 255. qed;
                        enum_lens[index - 1] as u8,
                        "Quit",
                        "",
                    );

                    let selected_option = prompt.interact()?;
                    if selected_option == enum_lens[index - 1] as u8 { break; }
                    selected_options.push(selected_option);
                )+


                let selected_name = input("").placeholder("Give it a name!").interact()?;
                index = 0;

                output.push(
                    ($({
                        // To expand the code we need at least one of the variables used in the repetition, otherwise the compilation fails.
                        $prompt_message;
                        index += 1;
                        // This is safe because `selected_option` is one among the discriminants of the enum variants, which are just 1-byte. qed;
                        let variant = unsafe{std::mem::transmute(selected_options[index - 1])};
                        variant
                    }),+,
                    selected_name)
                );
            }
            output
        }
    }
}

/// A macro to facilitate the select multiple variant of an enum and store them inside a Vec.
/// - `$enum`: The enum type to be iterated over for the selection. This enum must implement
///   `IntoEnumIterator` and `EnumMessage` traits from the `strum` crate. Each variant is
///   responsible of its own messages.
/// - `$prompt_message`: The message displayed to the user. It must implement the `Dispay` trait.
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
/// use strum_macros::{EnumIter, EnumMessage};
/// use strum::{IntoEnumIterator, EnumMessage};
/// use cliclack::{multiselect};
///
/// #[derive(Debug, EnumIter, EnumMessage, Copy, Clone)]
/// enum FieldlessEnum {
///     #[strum(message = "Type 1", detailed_message = "Detailed message for Type 1")]
///     Type1,
///     #[strum(message = "Type 2", detailed_message = "Detailed message for Type 2")]
///     Type2,
///     #[strum(message = "Type 3", detailed_message = "Detailed message for Type 3")]
///     Type3,
/// }
///
/// let vec = multiselect_pick!(FieldlessEnum, "Hello, world!");
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
	($enum: ty, $prompt_message: expr) => {{
		// Ensure the enum is 1-byte long. This is needed cause fieldless enums with > 256 elements
		// will lead to unexpected behavior as the conversion to u8 for them isn't detected as wrong
		// at compile time. Enums containing variants with fields will be catched at compile time.
		// Weird but possible.
		assert!(std::mem::size_of::<$enum>() == 1);
		let mut prompt = multiselect(format!(
			"{} {}",
			$prompt_message,
			"Pick an option by pressing the spacebar. Press enter when you're done!"
		))
		.required(false);

		for variant in <$enum>::iter() {
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
			.collect::<Vec<TemplatePalletConfigCommonTypes>>()
	}};
}

/// This function performs a loop allowing the user to input as many things as wanted and collecting
/// them into a `Vec`. In order to stop the loop, the user may enter Quit. # Parameters
///
/// - `prompt_message: &str`: The message shown to the user in each interaction.
pub(crate) fn collect_loop_cliclack_inputs(prompt_message: &str) -> anyhow::Result<Vec<String>> {
	let mut output = Vec::new();
	loop {
		let input: String =
			input(prompt_message).placeholder("If you're done, type 'quit'").interact()?;
		if input == "quit" {
			break;
		}
		output.push(input);
	}
	Ok(output)
}
