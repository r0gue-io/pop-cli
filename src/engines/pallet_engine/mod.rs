#![allow(unused)]
//! Pallet Engine - A set of tools to add pallets to your runtime
//! To add a pallet one usually needs to consider a few pieces of information together:
//!
//! 1. How is the pallet going to be configured? (impl pallet::Config for Runtime)
//! 2. How is the runtime instantiated in the current input source? (i.e. what kind of runtime declaration are we working with?)
//! 3. Are there benchmarks for the current pallet that would need to be included in the runtime and subsequently the runtime manifest? (including entries for list_benchmarks!)
//!
//! These are some necessary questions, but not an exhaustive list. One might further have to answer the following:
//! 1. Does adding this pallet exceed the total number of pallets > 255?
//! 2. Does the computed pallet index overflow the u8 bound? (gaps in explicit pallet index declarations)
//! 3. Is the pallet already in the runtime? If yes, do we add a second instance or abort?
//! 4. Does this pallet require a genesis configuration? 
//!
//! It is the goal of this module, to answer all of these questions.

mod pallet_entry;
mod parser;
mod template;

use anyhow::{anyhow, bail, Context};
use pallet_entry::Numbers;
use parser::RuntimeDeclaration;
use proc_macro2::TokenStream;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use syn::{spanned::Spanned, ItemMacro};
pub use template::{create_pallet_template, TemplatePalletConfig};

use pallet_entry::{AddPalletEntry, ReadPalletEntry};


/// The main entry point into the engine.
pub fn execute(pallet: AddPallet, runtime_path: PathBuf) -> anyhow::Result<()> {
    Ok(())
}


/// The Pallet Engine has two Paths `input` and `output`.
/// During processing, we want to keep the input source and the output source separate as
/// to have assurance that our references into the `input` file are still valid during construction of `output`.
/// Once fully processed, `output` can simply overwrite `input`
pub struct PalletEngine {
    /// Input source to PalletEngine - This will be the path to the runtime/src/lib.rs
    /// In code, this is never touched directly until the final step where it's overwritten
    /// Interim, it's treated as immutable data
    input: PathBuf,
    /// Stores the details of the runtime pallets built from input
    details: PalletDetails,
    /// This stores the path to the runtime file being processed
    /// User must have read/write permissions for potentially destructive editing
    /// All insertions are ultimately written here
    output: PathBuf,
    // /// This stores the path to the runtime manifest file runtime/Cargo.toml
    // manifest: PathBuf,
}

/// PalletDetails is data generated after parsing of a given `input` runtime file
/// This will make observations as to which pallets are there, if there are instances of the pallets
/// among details such as the span of the construct_runtime! macro, the type of runtime declaration etc.
struct PalletDetails {
    /// Number of pallets in the runtime, changes based on pallets added to output
    pallets: Vec<ReadPalletEntry>,
    /// construct_runtime! macro span start location. Any pallet that's being added
    /// should search, uptil this point to make sure existing pallets do not conflict
    crt_start: usize,
    /// construct_runtime! macro span end location.
    /// construct_runtime! macro span end location.
    /// For changes that happen after construct_runtime! is edited
    crt_end: usize,
    /// Total number of lines in input. Useful for inserting lines.
    file_end: usize,
    /// Type of runtime declaration being processed
    declaration: RuntimeDeclaration,
}
// Public API
impl PalletEngine {
    /// Query the output path
    pub fn output(&self) -> &Path {
        &self.output
    }
    /// Create a new PalletEngine
    pub fn new(input: PathBuf) -> anyhow::Result<Self> {
        let tmp_dir = tempfile::TempDir::new()?;
        let output : PathBuf = tmp_dir.path().join("lib.rs");
        // Open the file specified in `output`. If non-empty, delete its contents.
        if output.exists() && output.is_file() {
            std::fs::remove_file(output.as_path())?;
        }
        File::create(output.as_path()).context(format!(
            "Failed to create PalletEngine with output: {}",
            output.display()
        ))?;
        // Build Pallet Details
        let __buf = BufReader::new(File::open(&input)?);
        let file_end = __buf.lines().count();
        let rt = fs::read_to_string(&input)?;
        let ast: syn::File = syn::parse_file(rt.as_ref())?;
        let mut details = Option::<PalletDetails>::None;
        for item in ast.items.iter() {
            match item {
                syn::Item::Macro(ItemMacro { mac, .. }) => {
                    if let Some(mac_id) = mac.path.get_ident() {
                        if mac_id == "construct_runtime" {
                            let (crt_start, crt_end) =
                                (item.span().start().line, item.span().end().line);
                            let declaration =
                                mac.parse_body::<RuntimeDeclaration>().map_err(|e| {
                                    anyhow!("Cannot parse construct_runtime from input").context(e)
                                })?;
                            let pallets = Self::build_pallet_details(&declaration)?;
                            details = Some(PalletDetails {
                                pallets,
                                crt_start,
                                crt_end,
                                file_end,
                                declaration,
                            });
                        }
                    }
                }
                _ => {}
            };
        }
        let Some(details) = details else {
            bail!("No pallets/construct_runtime! found in input");
        };
        Ok(Self {
            input,
            output,
            details,
        })
    }
    /// Helper for PalletEngine::new, Builds pallet details from the construct_runtime! macro
    fn build_pallet_details(
        declaration: &RuntimeDeclaration,
    ) -> anyhow::Result<Vec<ReadPalletEntry>> {
        // Instance map to track the number of current instances for a pallet entry
        let mut imap: HashMap<String, u8> = HashMap::new();
        let mut pe: HashMap<String, ReadPalletEntry> = HashMap::new();
        let pallet_entries = match declaration {
            RuntimeDeclaration::Implicit(i) => {
                for pallet in i.pallets.iter() {
                    let entry: String = pallet.span.source_text().unwrap();
                    let entry = entry.split(':').next().unwrap().to_string();
                    let index = pallet.index;
                    if pallet.instance.is_some() {
                        // If using instance syntax i.e. pallet::<Instance> set instance to 1
                        imap.entry(entry.clone())
                            .and_modify(|e| *e += 1)
                            .or_insert(1);
                    };
                    pe.entry(entry.clone())
                        .and_modify(|e| {
                            let v = imap.get(&entry).unwrap_or(&0);
                            e.numbers.instance = *v;
                        })
                        .or_insert(ReadPalletEntry {
                            entry: entry.to_string(),
                            numbers: Numbers { index, instance: 0 },
                        });
                }
                pe.into_iter().map(|e| e.1).collect()
            }
            RuntimeDeclaration::Explicit(e) | RuntimeDeclaration::ExplicitExpanded(e) => {
                for pallet in e.pallets.iter() {
                    let entry: String = pallet.span.source_text().unwrap();
                    let entry = entry.split(':').next().unwrap().to_string();
                    let index = Some(pallet.index);
                    if pallet.instance.is_some() {
                        // If using instance syntax i.e. pallet::<Instance> set instance to 1
                        imap.entry(entry.clone())
                            .and_modify(|e| *e += 1)
                            .or_insert(1);
                    };
                    pe.entry(entry.clone())
                        .and_modify(|e| {
                            let v = imap.get(&entry).unwrap_or(&0);
                            e.numbers.instance = *v;
                        })
                        .or_insert(ReadPalletEntry {
                            entry: entry.to_string(),
                            numbers: Numbers { index, instance: 0 },
                        });
                }
                pe.into_iter().map(|e| e.1).collect()
            }
        };
        Ok(pallet_entries)
    }
}

// Private methods for internal use.
impl PalletEngine {
    /// Add `n` line-breaks to output
    fn add_new_line(&self, n: usize) -> anyhow::Result<()> {
        let mut file = OpenOptions::new().append(true).open(&self.output)?;
        let newlines: String = std::iter::repeat('\n').take(n).collect();
        Ok(file.write_all(format!("{newlines}").as_bytes())?)
    }
    /// Append raw tokens to `output` file
    fn append_tokens(&self, tokens: TokenStream) -> anyhow::Result<()> {
        let content = prettyplease::unparse(&syn::parse_file(&tokens.to_string())?);
        let mut file = OpenOptions::new().append(true).open(&self.output)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }
    /// Append string as is to `output` file
    fn append_str(&self, content: String) -> anyhow::Result<()> {
        let mut file = OpenOptions::new().append(true).open(&self.output)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }
    /// Append lines [start..end] from `input` source to `output`.
    /// Typically used to scaffold the `output` before and after making changes
    fn append_lines_from(&self, start: usize, end: usize) -> anyhow::Result<()> {
        let file = File::open(self.input.as_path())?;
        let reader = BufReader::new(file);
        // Assuming a worst case of 150 chars per line which is almost never the case in a typical substrate runtime file
        // In the kitchensink node the maximum line length is 138 chars wide
        let mut snip = String::with_capacity(150 * (end - start));
        let mut current_line = 0;

        for line in reader.lines() {
            current_line += 1;

            if current_line < start {
                // Skip lines until start
                continue;
            } else if current_line > end {
                // Stop reading after end
                break;
            }

            snip.push_str(&line?);
            snip.push('\n');
        }
        let mut file = OpenOptions::new().append(true).open(&self.output)?;
        file.write_all(snip.as_bytes())?;
        Ok(())
    }
    /// Insert string at line. Errors if line number doesn't exist in `output`
    fn insert_at(&self, line: usize, str: &str) -> anyhow::Result<()> {
        let reader = BufReader::new(File::open(&self.output)?);
        let mut temp_file = tempfile::NamedTempFile::new()?;
        let temp_path = &temp_file.path().to_path_buf();
        let mut written = false;
        for (idx, l) in reader.lines().enumerate() {
            if idx == line {
                writeln!(temp_file, "{str}")?;
                written = true;
            }
            writeln!(temp_file, "{}", l?)?;
        }
        fs::rename(temp_path, &self.output)?;
        if !written {
            bail!("PalletEngine output doesn't have line {line} to insert at");
        }
        Ok(())
    }
    /// Insert raw string at construct_runtime! Fails if there's no construct_runtime! in `output`
    /// By default inserts at the end of the macro
    fn insert_str_runtime(&self, str: &str) -> anyhow::Result<()> {
        let runtime_contents = fs::read_to_string(&self.output)?;
        let ast: syn::File = syn::parse_file(runtime_contents.as_ref())?;
        let mut runtime_found = false;
        for item in ast.items.iter() {
            match item {
                syn::Item::Macro(syn::ItemMacro { mac, .. }) => {
                    if let Some(mac_id) = mac.path.get_ident() {
                        if mac_id == "construct_runtime" {
                            runtime_found = true;
                            let r: RuntimeDeclaration = mac.parse_body().map_err(|err| {
                                anyhow!("PalletEngine output is in not parseable").context(err)
                            })?;
                            match r {
                                RuntimeDeclaration::Implicit(i) => {
                                    let ultimate = i
                                        .pallets
                                        .last()
                                        // This may be handled in the case that there's no pallets in CRT to begin with
                                        // And subsequent pallets will be added
                                        .expect("Fatal: No pallets defined in construct_runtime!")
                                        .clone();
                                    let end = ultimate.span.end().line;
                                    self.insert_at(end, str)?;
                                }
                                RuntimeDeclaration::Explicit(e)
                                | RuntimeDeclaration::ExplicitExpanded(e) => {
                                    let end = e
                                        .pallets
                                        .last()
                                        .expect("Fatal: No pallets defined in construct_runtime!")
                                        .span
                                        .end()
                                        .line;
                                    self.insert_at(end, str)?;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if !runtime_found {
            // Should never happen
            panic!("Construct Runtime not found in PalletEngine output. Cannot add pallet");
        }
        Ok(())
    }
    /// Add a new pallet to RuntimeDeclaration and return it.
    /// Used to pass a typed AddPallet struct to modify the RuntimeDeclaration
    fn add_pallet_runtime(&mut self, new_pallet: AddPalletEntry) -> anyhow::Result<()> {
        match &mut self.details.declaration {
            RuntimeDeclaration::Implicit(i) => {
                let mut ultimate = i
                    .pallets
                    .last()
                    .expect("Fatal: No pallets defined in construct_runtime!")
                    .clone();
                ultimate.index = new_pallet.index;
                ultimate.path.inner.segments[0].ident = new_pallet.path;
                ultimate.name = new_pallet.name;
                // println!("Ultimate pallet: {:?}", ultimate);
                i.pallets.push(ultimate);
                Ok(())
            }
            RuntimeDeclaration::Explicit(e) => {
                todo!()
            }
            RuntimeDeclaration::ExplicitExpanded(e) => {
                todo!()
            }
        }
    }
}
