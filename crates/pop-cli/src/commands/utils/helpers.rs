use cliclack::input;

/// A macro to facilitate the selection of an option among some pre-selected ones (represented as variants from a fieldless enum) and the naming of that selection. This process is repetead until the user stops it and then the result is stored in a `Vec` of tuples, containing the options and the given names. This is useful in processes such as designing a pallet's storage to generate a pallet template: we need the user to pick among the different storage types (StorageValue, StorageMap, StorageDoubleMap,...) and give names to those storages.
/// This macro utilizes the `cliclack` crate for the selection
/// and input prompts, and it requires the `strum` crate for enum iteration and messages.
///
/// # Parameters
///
/// - `$enum`: The enum type to be iterated over for selection. This enum must implement
///   `IntoEnumIterator` and `EnumMessage` traits from the `strum` crate. Each variant is responsible of its own messages.
/// - `$vec`: The vector to store the `(enum variant, String)` tuples.
/// - `$prompt_message`: The message displayed to the user at the very beginning of each iteration. Must implement the `Dispay` trait.
/// # Note
/// This macro only works with a 1-byte sized enum, this is, a fieldless enum with at most 255 elements. This is because we're just interested in letting the user to pick among a pre-defined set of options and then naming the selection. Instead of using a `Vec` of tuples we may have used enums whose variants contain a `String`, but this is worse for several reasons: 
/// 1. The size of the enum would take at least 32 bytes, instead of the single byte used here.
/// 2. This macro will be likely used to determine the option chosen by the user and to write a template according to that selection. To pass that information together with the name to the `rs.templ` file, we would need to parse it before, so using a `Vec` of tuples it's already parsed for us!
/// 3. Using fieldless enums, identify each variant with bytes is straightforward, so we can play with `u8` all the time and just recover the variant at the end. This allows us to add the option of quitting the loop as a number, instead of adding it to all the enums using this macro.
///
/// The decision of using 1-byte enum instead of fieldless enums is for simplicity: we won't probably offer a user to pick from > 256 options. If this macro is used with enums containing fields, the conversion to `u8` will simply be detected at compile time and the compilation will fail. If this macro is used with fieldless enums greater than 1-byte (really weird but possible), the conversion to u8 may lead to unexpected behavior, so we panic at runtime if that happens for completeness.
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
/// let mut my_vec = Vec::new();
///
/// enum_named_selector!(FieldlessEnum, my_vec, "Hello world!");
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
/// Additionally, this macro handle results, so it must be used inside a function doing so. Otherwise the compilation will fail.
#[macro_export]
macro_rules! unit_enum_named_selector{
    ($enum: ty, $vec: ident, $prompt_message: expr) => {
        // Ensure the enum is 1-byte long. This is needed cause fieldless enums with >=257 elements will lead to unexpected behavior as the conversion to u8 for them isn't detected as wrong at compile time. Weird but possible
        assert!(std::mem::size_of::<$enum>() == 1);
        let enum_len = <$enum>::iter().collect::<Vec<_>>().len();
        // Ensure that there's at least a free spot for quitting the loop. Weird but possible.
        assert!(enum_len != 255);
        // Now it can be happily converted to u8.
        let enum_len = enum_len as u8;
        loop{
            let mut prompt = select($prompt_message).initial_value(0u8);
            for variant in <$enum>::iter(){
                prompt = prompt.item(
                    variant as u8,
                    variant.get_message().unwrap_or_default(),
                    variant.get_detailed_message().unwrap_or_default(),
                );
            };

            prompt = prompt.item(
                enum_len,
                "Quit",
                "",
            );

            let selected_option = prompt.interact()?;
            if selected_option == enum_len { break; }

            let selected_name = input("").placeholder("Give it a name!").interact()?;
            
            // This is safe because `selected_option` is one among the discriminants of the enum variants, which are just 1-byte. qed;
            $vec.push((unsafe{std::mem::transmute(selected_option)}, selected_name));
        }
    }
}

/// This function performs a loop allowing the user to input as many things as wanted and collecting them into a `Vec`. In order to stop the loop, the user may enter Quit.
/// # Parameters
///
/// - `prompt_message: &str`: The message shown to the user in each interaction.
pub fn collect_loop_cliclack_inputs(prompt_message: &str) -> Vec<String>{
    let mut output = Vec::new();
    loop{
        let input = input(prompt_message).placeholder("If you're done, type 'Quit'").interact()?;
        if input=="Quit"{break;}
        output.push(input.to_string());
    }
    output
}