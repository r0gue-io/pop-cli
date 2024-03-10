use super::{pallet_entry::AddPalletEntry, Dependency, Features, PalletEngine, State, TomlEditor};
use crate::commands::add::AddPallet;
use anyhow::Result;
use log::{error, warn};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use Steps::*;
/// Define the steps needed for a particular pallet insertion
/// As of now, there's no clean solution to find out the newlines (\n) in a TokenStream
/// so variants expecting Tokenstreams accompany a line count datum for PalletEngine cursor.
#[derive(Debug)]
pub(super) enum Steps {
	/// Import statements for pallet
	RuntimePalletImport((TokenStream2, usize)),
	/// Every pallet must impl pallet::Config for Runtime
	RuntimePalletConfiguration((TokenStream2, usize)),
	/// The runtime/Cargo.toml needs an import for the pallet being inserted
	/// This includes features [try-runtime, runtime-benchmarks, std], path information for `git` or local path
	RuntimePalletDependency(Dependency),
	/// ConstructRuntime! entry
	ConstructRuntimeEntry(AddPalletEntry),
	/// Include a `list_benchmarks!` entry
	ListBenchmarks(String),
	/// Does pallet require a genesis configuration?
	ChainspecGenesisConfig(String),
	/// ChainSpec imports if necessary
	ChainspecGenesisImport((TokenStream2, usize)),
	/// Node specific imports if the above two are required
	NodePalletDependency(Dependency),
	/// PalletEngine State transitions
	SwitchTo(State),
}
/// Some rules to follow when constructing steps:
/// The pallet engine state expects to go as edits would, i.e. top to bottom lexically
/// So it makes sense for any given file, to first include an import, then items that refer to it
/// In case of a pallet, you'd always put `RuntimePalletImport`, `RuntimePalletConfiguration`, `ConstructRuntimeEntry` in that order.
pub(super) fn step_builder(pallet: AddPallet) -> Result<Vec<Steps>> {
	let mut steps: Vec<Steps> = vec![];
	match pallet {
		// Adding a pallet-parachain-template requires 5 distinct steps
		AddPallet::Template => {
			// steps.push(RuntimePalletDependency(Dependency::runtime_template()));
			steps.push(RuntimePalletImport((
				quote!(
					// Imports by pop-cli
					pub use pallet_parachain_template;
				),
				3,
			)));
			steps.push(SwitchTo(State::Config));
			steps.push(RuntimePalletConfiguration((
				quote!(
					/// Configure the pallet template in pallets/template.
					impl pallet_parachain_template::Config for Runtime {
						type RuntimeEvent = RuntimeEvent;
					}
				),
				5,
			)));
			steps.push(SwitchTo(State::ConstructRuntime));
			steps.push(ConstructRuntimeEntry(AddPalletEntry::new(
				// Index - None, means Pallet Engine will automatically compute an index
				None,
				// Path
				"pallet_parachain_template",
				// Pallet name
				// TODO (high priority): implement name conflict resolution strategy
				"Template",
			)));
			// steps.push(NodePalletDependency(Dependency::node_template()))
		},
		AddPallet::Frame(_) => unimplemented!("Frame pallets not yet implemented"),
	};
	Ok(steps)
}
/// Execute steps on PalletEngine.
/// Each execution edits a file.
/// Sequence of steps matters so take care when ordering them
/// Works only for Template pallets at the moment.. See config and CRT inserts
pub(super) fn run_steps(mut pe: PalletEngine, mut te: TomlEditor, steps: Vec<Steps>) -> Result<()> {
	use super::State::*;
	pe.prepare_output()?;
	for step in steps.into_iter() {
		match step {
			RuntimePalletDependency(dep) => te.inject_runtime(dep)?,
			RuntimePalletImport(stmt) => {
				match pe.state {
					Init => {
						warn!("`prepare_output` was not called");
						pe.state = Import;
						pe.insert_import(stmt);
					},
					Import => pe.insert_import(stmt)?,
					_ => {
						// We don't support writing import statements in any other engine state
						// Log non-fatal error and continue
						error!(
							"Cannot write import stmts in state {0:?}. Check step builder",
							pe.state
						);
						continue;
					},
				};
			},
			SwitchTo(State::Config) => pe.prepare_config()?,
			RuntimePalletConfiguration(config) => {
				if pe.state != Config {
					// Not really a fatal error, but may cause unexpected behaviour
					warn!("Engine not in Config state, executing config insertion anyways");
				}
				pe.insert_config(config)?
			},
			SwitchTo(State::ConstructRuntime) => pe.prepare_crt()?,
			ConstructRuntimeEntry(entry) => pe.add_pallet_runtime(entry)?,
			// ListBenchmarks(step) => pe.insert(step),
			// ListBenchmarks(step) => pe.insert(step),
			// ChainspecGenesisConfig(step) => pe.insert(step),
			// ChainspecGenesisImport(step) => pe.insert(step),
			NodePalletDependency(dep) => te.inject_node(dep)?,
			step => {
				unimplemented!("{step:?} unimplemented")
			},
		}; // -- match --
	} // -- for --
  // Finalize runtime edits
	pe.merge()?;
	// TODO: Finalize toml and chainspec edits
	Ok(())
}
