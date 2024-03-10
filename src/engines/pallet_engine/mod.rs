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
mod steps;
mod template;

use crate::commands::add::AddPallet;
use anyhow::{anyhow, bail, Context};
use dependency::{Dependency, Features};
use log::warn;
use pallet_entry::Numbers;
use pallet_entry::{AddPalletEntry, ReadPalletEntry};
use parser::RuntimeDeclaration;
use proc_macro2::TokenStream;
use quote::quote;
use std::{
	collections::HashMap,
	fs::{self, File, OpenOptions},
	io::{BufRead, BufReader, Write},
	path::{Path, PathBuf},
};
use steps::{run_steps, step_builder};
use syn::{spanned::Spanned, Item, ItemMacro};
pub use template::{create_pallet_template, TemplatePalletConfig};

/// The main entry point into the engine.
pub fn execute(pallet: AddPallet, runtime_path: PathBuf) -> anyhow::Result<()> {
	let mut pe = PalletEngine::new(&runtime_path)?;
	// Todo: move logic to sep. function. Add option to source from cli
	let runtime_manifest = &runtime_path.parent().unwrap().join("Cargo.toml");
	let node_manifest = &runtime_path.parent().unwrap().parent().unwrap().join("node/Cargo.toml");
	let dep = TomlEditor { runtime: runtime_manifest.to_owned(), node: node_manifest.to_owned() };
	let steps = step_builder(pallet)?;
	run_steps(pe, dep, steps)
}

struct TomlEditor {
	// workspace
	runtime: PathBuf,
	node: PathBuf,
}
impl TomlEditor {
	fn inject_node(&self, dep: Dependency) -> anyhow::Result<()> {
		todo!()
	}
	fn inject_runtime(&self, dep: Dependency) -> anyhow::Result<()> {
		todo!()
	}
}

/// State of PalletEngine at any given moment in time
#[derive(Debug, Default, PartialEq)]
// TODO: Impl sequence checking through discriminants
enum State {
	#[default]
	Init,
	Import,
	Config,
	ConstructRuntime,
	Benchmarks,
	ImplRuntimeApi,
}
/// The Pallet Engine has two Paths `input` and `output`.
/// During processing, we keep the input source as read only and perform all processing into the `output` sink
/// This allows for manual checking using a diff tool that the edits performed are indeed satisfactory before calling merge
///  which will overwrite `input` with the processed `output`
pub struct PalletEngine {
	/// Input source to PalletEngine - This will be the path to the runtime/src/lib.rs
	/// In code, this is never touched directly until the final step where it's overwritten
	/// Interim, it's treated as immutable data
	input: PathBuf,
	/// Stores the details of the runtime pallets built from input
	details: PalletDetails,
	/// Stores imports necessary to make new items available
	imports: ImportDetails,
	/// This stores the path to the runtime file being processed
	/// User must have read/write permissions for potentially destructive editing
	/// All insertions are ultimately written here
	output: PathBuf,
	// /// This stores the path to the runtime manifest file runtime/Cargo.toml
	// manifest: PathBuf,
	/// State
	state: State,
	/// Cursor for tracking where we are in the output
	cursor: usize,
}
impl Drop for PalletEngine {
	fn drop(&mut self) {
		let output_dir = self.output.parent().unwrap();
		let _ = fs::remove_dir_all(output_dir);
	}
}

/// PalletDetails is data generated after parsing of a given `input` runtime file
/// This will make observations as to which pallets are there, if there are instances of the pallets
/// among details such as the span of the construct_runtime! macro, the type of runtime declaration etc.
struct PalletDetails {
	/// Number of pallets in the runtime, changes based on pallets added to output
	pallets: Vec<ReadPalletEntry>,
	/// construct_runtime! macro span start location. Any pallet that's being added
	/// should search, uptil this point to make sure existing pallets do not conflict
	/// Note: Read-only from self.input
	crt_start: usize,
	/// construct_runtime! macro span end location.
	/// For changes that happen after construct_runtime! is edited
	/// Note: Read-only from self.input
	crt_end: usize,
	/// Total number of lines in input. Useful for inserting lines.
	file_end: usize,
	/// Type of runtime declaration being processed
	declaration: RuntimeDeclaration,
}
struct ImportDetails {
	/// On reading the source file, we obtain `last_import` which is the ending line of final import
	/// statement, and also from where additional pallet imports must be added
	last_import: usize,
}
// Public API
impl PalletEngine {
	/// Query the output path
	pub fn output(&self) -> &Path {
		&self.output
	}
	/// Consume self merging `output` and `input`
	/// Call this to finalize edits
	pub fn merge(mut self) -> anyhow::Result<()> {
		// TODO: since we are not interacting with any post-CRT items, this is ok
		&mut self.append_lines_from(self.details.crt_end + 1, self.details.file_end)?;
		fs::copy(&self.output, &self.input)?;
		fs::remove_file(&self.output);
		Ok(())
	}
	/// Create a new PalletEngine
	pub fn new(input: &PathBuf) -> anyhow::Result<Self> {
		let tmp_dir = PathBuf::from(format!("/tmp/pallet_engine_{}", uuid::Uuid::new_v4()));
		fs::create_dir(&tmp_dir)
			.context("Failed to create temporary directory for PalletEngine")?;
		let output: PathBuf = tmp_dir.join("out_lib.rs");
		// Open the file specified in `output`. If non-empty, delete its contents.
		if output.exists() && output.is_file() {
			std::fs::remove_file(output.as_path())?;
		}
		File::create(&output)
			.context(format!("Failed to create PalletEngine with output: {}", output.display()))?;
		// Build Pallet Details
		let __buf = BufReader::new(File::open(&input)?);
		let file_end = __buf.lines().count();
		let rt = fs::read_to_string(&input)?;
		let ast: syn::File = syn::parse_file(rt.as_ref())?;
		let mut details = Option::<PalletDetails>::None;
		let mut last_import = None;
		let mut _macro_cross = false;
		for item in ast.items.iter() {
			match item {
				Item::Use(_) => {
					// Fetch last import
					// Note, other use statements are present inside modules in a standard runtime
					// Additional safety mechanism to make sure pallet-imports are always before construct_runtime!
					if !_macro_cross {
						last_import = Some(item.span().end().line);
					}
				},
				Item::Macro(ItemMacro { mac, .. }) => {
					if let Some(mac_id) = mac.path.get_ident() {
						if mac_id == "construct_runtime" {
							_macro_cross = true;
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
				},
				_ => {},
			};
		}
		let imports =
			ImportDetails { last_import: last_import.expect("Imports are always present") };
		let Some(details) = details else {
			bail!("No pallets/construct_runtime! found in input");
		};
		Ok(Self {
			input: input.to_owned(),
			imports,
			output,
			details,
			state: State::Init,
			cursor: 0,
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
						imap.entry(entry.clone()).and_modify(|e| *e += 1).or_insert(1);
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
			},
			RuntimeDeclaration::Explicit(e) | RuntimeDeclaration::ExplicitExpanded(e) => {
				for pallet in e.pallets.iter() {
					let entry: String = pallet.span.source_text().unwrap();
					let entry = entry.split(':').next().unwrap().to_string();
					let index = Some(pallet.index);
					if pallet.instance.is_some() {
						// If using instance syntax i.e. pallet::<Instance> set instance to 1
						imap.entry(entry.clone()).and_modify(|e| *e += 1).or_insert(1);
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
			},
		};
		Ok(pallet_entries)
	}
}

// Private methods for internal use.
// Note: Some methods update `self.cursor` and they take exclusive refs (&mut self)
// For functions which don't do that (i.e. take ref by &self), the caller must decide how to increment cursor
// This is relevant when calling methods like `append_tokens` or `append_str` which append single and multi-line strs
// without analyzing it for newlines.
#[allow(unused)]
impl PalletEngine {
	/// Prepare `output` by first, adding pre-CRT items such as imports and modules
	/// Then adding the construct_runtime! macro
	/// And finally adding the post-CRT items such as benchmarks, impl_runtime_apis! and so forth
	fn prepare_output(&mut self) -> anyhow::Result<()> {
		if self.state != State::Init {
			bail!("PalletEngine is not in Init stage, cursor: {}", self.cursor);
		} else {
			// First pre-CRT items - imports
			self.append_lines_from(0, self.imports.last_import)?;

			self.state = State::Import;
			Ok(())
		}
	}
	/// Prepare `output` for taking new pallet configurations
	fn prepare_config(&mut self) -> anyhow::Result<()> {
		if self.state != State::Import {
			bail!("PalletEngine is not in Import stage, cursor: {}", self.cursor);
		} else {
			self.append_lines_from(self.imports.last_import + 1, self.details.crt_start - 1);
			self.state = State::Config;
			Ok(())
		}
	}
	/// Prepare `output` for CRT items
	fn prepare_crt(&mut self) -> anyhow::Result<()> {
		if self.state != State::Config {
			bail!("PalletEngine is not in Config stage, cursor: {}", self.cursor);
		} else if self.state == State::ConstructRuntime {
			warn!("PalletEngine is already in ConstructRuntime stage, cursor: {}", self.cursor);
			return Ok(());
		}
		self.add_new_line(1)?;
		self.append_lines_from(self.details.crt_start, self.details.crt_end);
		self.state = State::ConstructRuntime;
		Ok(())
	}
	/// Add `n` line-breaks to output
	fn add_new_line(&mut self, n: usize) -> anyhow::Result<()> {
		let mut file = OpenOptions::new().append(true).open(&self.output)?;
		let newlines: String = std::iter::repeat('\n').take(n).collect();
		let rs = file.write_all(format!("{newlines}").as_bytes())?;
		self.cursor += 1;
		Ok(rs)
	}
	/// Append raw tokens to `output` file, cursor should be handled by caller
	fn append_tokens(&self, tokens: TokenStream) -> anyhow::Result<()> {
		let content = prettyplease::unparse(&syn::parse_file(&tokens.to_string())?);
		let mut file = OpenOptions::new().append(true).open(&self.output)?;
		file.write_all(content.as_bytes())?;
		Ok(())
	}
	/// Append string as is to `output` file, cursor should be handled by caller
	fn append_str(&self, content: String) -> anyhow::Result<()> {
		let mut file = OpenOptions::new().append(true).open(&self.output)?;
		file.write_all(content.as_bytes())?;
		Ok(())
	}
	/// Insert import statement
	/// As of now, it's imperative to call this function for pre-CRT item insertions
	/// The correctness of calling this function depends on the `state` of PalletEngine
	/// and the step_runner makes sure that it will only call this function when State is either
	/// `State::Init` or `State::Import`
	fn insert_import(&mut self, import_stmt: (TokenStream, usize)) -> anyhow::Result<()> {
		self.append_tokens(import_stmt.0);
		self.imports.last_import += import_stmt.1;
		Ok(())
	}
	/// Insert configuartion for a pallet - only for pallet-template atm
	fn insert_config(&mut self, config: (TokenStream, usize)) -> anyhow::Result<()> {
		self.append_tokens(config.0);
		self.cursor += config.1;
		Ok(())
	}
	/// Append lines [start..end] from `input` source to `output`.
	/// Typically used to scaffold the `output` before and after making changes
	/// Increment cursor by exactly the number of lines inserted
	fn append_lines_from(&mut self, start: usize, end: usize) -> anyhow::Result<()> {
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
		let mut file = OpenOptions::new()
			.append(true)
			.open(&self.output)
			.context("fn append_lines_from - cannot open output")?;
		file.write_all(snip.as_bytes())?;
		self.cursor += end - start;
		Ok(())
	}
	/// Same as `append_lines_from` but doesn't update cursor
	fn append_lines_from_no_update(&mut self, start: usize, end: usize) -> anyhow::Result<()> {
		self.append_lines_from(start, end)?;
		self.cursor -= (end - start);
		Ok(())
	}
	/// Insert string at line. Errors if line number doesn't exist in `output`
	/// cursor should be handled by caller
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
	/// By default inserts at the end of the macro.
	/// Note: cursor is only incremented by one, so `str` is expected to be a single line
	fn insert_str_runtime(&mut self, str: &str) -> anyhow::Result<()> {
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
								},
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
								},
							}
						}
					}
				},
				_ => {},
			}
		}
		if !runtime_found {
			// Should never happen
			panic!("Construct Runtime not found in PalletEngine output. Cannot add pallet");
		}
		// Typically insert_str_runtime would be used to insert a single pallet entry so this is okay
		self.cursor += 1;
		Ok(())
	}
	/// Add a new pallet to RuntimeDeclaration and return it.
	/// Used to pass a typed AddPallet to modify the RuntimeDeclaration
	/// Overwrites existing CRT in output, this means that `self.cursor` has to backtracked to crt_start
	/// and updated after the pallet entry has been inserted
	fn add_pallet_runtime(&mut self, new_pallet: AddPalletEntry) -> anyhow::Result<()> {
		let mut entry = String::new();
		let AddPalletEntry { name, path, index } = new_pallet;
		if let Some(idx) = index {
			entry = format!("\t\t{}: {} = {},", name, path, idx);
		} else {
			entry = format!("\t\t{}: {},", name, path);
		}
		self.insert_str_runtime(&entry)?;
		// match &mut self.details.declaration {
		// 	RuntimeDeclaration::Implicit(i) => {
		// 		let mut ultimate = i
		// 			.pallets
		// 			.last()
		// 			.ok_or(anyhow!("Fatal: No pallets defined in construct_runtime!"))?
		// 			.clone();
		// 		ultimate.index = new_pallet.index;
		// 		ultimate.path.inner.segments[0].ident = new_pallet.path;
		// 		ultimate.name = new_pallet.name;
		// 		i.pallets.push(ultimate);
		// 	},
		// 	RuntimeDeclaration::Explicit(e) => {
		// 		todo!()
		// 	},
		// 	RuntimeDeclaration::ExplicitExpanded(e) => {
		// 		todo!()
		// 	},
		// };
		Ok(())
	}
}
// TODO
mod dependency {
	use strum_macros::{Display, EnumString};

	#[derive(EnumString, Display, Debug)]
	pub(in crate::engines::pallet_engine) enum Features {
		#[strum(serialize = "std")]
		Std,
		#[strum(serialize = "runtime-benchmarks")]
		RuntimeBenchmarks,
		#[strum(serialize = "try-runtime")]
		TryRuntime,
		Custom(String),
	}
	#[derive(Debug)]
	pub(in crate::engines::pallet_engine) struct Dependency {
		features: Vec<Features>,
		path: String,
		no_default_features: bool,
	}

	impl Dependency {
		/// Dependencies required for adding a pallet-parachain-template to runtime
		pub(in crate::engines::pallet_engine) fn runtime_template() -> Self {
			Self {
				features: vec![Features::RuntimeBenchmarks, Features::TryRuntime, Features::Std],
				// TODO hardcode for now
				path: format!(r#"path = "../pallets/template""#),
				no_default_features: true,
			}
		}
		/// Dependencies required for adding a pallet-parachain-template to node
		pub(in crate::engines::pallet_engine) fn node_template() -> Self {
			Self {
				features: vec![Features::RuntimeBenchmarks, Features::TryRuntime],
				// TODO hardcode for now
				path: format!(r#"path = "../pallets/template""#),
				no_default_features: false,
			}
		}
	}
}
