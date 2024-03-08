use super::{pallet_entry::AddPalletEntry, PalletEngine};
use crate::commands::add::AddPallet;
use anyhow::Result;
use dependency::Dependency;
use log::{error, warn};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use Steps::*;
use super::State;
/// Define the steps needed for a particular pallet insertion
#[derive(Debug)]
pub(super) enum Steps {
    /// Import statements for pallet
    RuntimePalletImport(TokenStream2),
    /// Every pallet must impl pallet::Config for Runtime
    RuntimePalletConfiguration(TokenStream2),
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
    ChainspecGenesisImport(TokenStream2),
    /// Node specific imports if the above two are required
    NodePalletDependency(Dependency),
    /// PalletEngine State transitions
    SwitchTo(State),
}

macro_rules! steps {
    ($cmd:expr) => {
        steps.push($cmd);
    };
}
/// Some rules to follow when constructing steps:
/// The pallet engine state expects to go as edits would, i.e. top to bottom lexically
/// So it makes sense for any given file, to first include an import, then items that refer to it
/// In case of a pallet, you'd always put `RuntimePalletImport`, `RuntimePalletConfiguration`, `ConstructRuntimeEntry` sin that order.
pub(super) fn step_builder(pallet: AddPallet) -> Result<Vec<Steps>> {
    let mut steps: Vec<Steps> = vec![];
    match pallet {
        // Adding a pallet-parachain-template requires 5 distinct steps
        AddPallet::Template => {
            // steps.push(RuntimePalletDependency(Dependency::runtime_template()));
            steps.push(RuntimePalletImport(quote!(
                pub use pallet_parachain_template;
            )));
            steps.push(SwitchTo(State::Config));
            steps.push(RuntimePalletConfiguration(quote!(
                /// Configure the pallet template in pallets/template.
                impl pallet_parachain_template::Config for Runtime {
                    type RuntimeEvent = RuntimeEvent;
                }
            )));
            steps.push(SwitchTo(State::ConstructRuntime));
            steps.push(ConstructRuntimeEntry(AddPalletEntry::new(
                // Index
                None,
                // Path
                "pallet_parachain_template",
                // Pallet name
                // TODO (high priority): implement name conflict resolution strategy
                "Template",
            )));
            // steps.push(NodePalletDependency(Dependency::node_template()))
        }
        AddPallet::Frame(_) => unimplemented!("Frame pallets not yet implemented"),
    };
    Ok(steps)
}
/// Execute steps on PalletEngine.
/// Each execution edits a file.
/// Sequence of steps matters so take care when ordering them
/// Works only for Template pallets at the moment.. See config and CRT inserts
pub(super) fn run_steps(mut pe: PalletEngine, steps: Vec<Steps>) -> Result<()> {
    use super::State::*;
    pe.prepare_output()?;
    for step in steps.into_iter() {
        match step {
            // RuntimePalletDependency(step) => pe.insert(step),
            RuntimePalletImport(stmt) => {
                match pe.state {
                    Init => {
                        warn!("`prepare_output` was not called");
                        pe.state = Import;
                        pe.insert_import(stmt);
                    }
                    Import => pe.insert_import(stmt)?,
                    _ => {
                        // We don't support writing import statements in any other engine state
                        // Log non-fatal error and continue
                        error!(
                            "Cannot write import stmts in state {0:?}. Check step builder",
                            pe.state
                        );
                        continue;
                    }
                };
            }
            SwitchTo(State::Config) => pe.prepare_config()?,
            RuntimePalletConfiguration(config) => {
                if pe.state != Config {
                    // Not really a fatal error, but may cause unexpected behaviour
                    warn!("Engine not in Config state, executing config insertion anyways");
                }
                pe.insert_config(config)?
            }
            SwitchTo(State::ConstructRuntime) => pe.prepare_crt()?,
            ConstructRuntimeEntry(_entry) => {
                // TODO : Switch to add_pallet_runtime
                // pe.add_pallet_runtime(entry)?
                pe.insert_str_runtime("\t\tTemplate: pallet_parachain_template = 100,")?;
            }
            // ListBenchmarks(step) => pe.insert(step),
            // ListBenchmarks(step) => pe.insert(step),
            // ChainspecGenesisConfig(step) => pe.insert(step),
            // ChainspecGenesisImport(step) => pe.insert(step),
            // NodePalletDependency(step) => pe.insert(step),
            step => {
                unimplemented!("{step:?} unimplemented")
            }
        }; // -- match --
    } // -- for --
    // Finalize runtime edits 
    pe.merge()?;
    // TODO: Finalize toml and chainspec edits
    Ok(())
}

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
                features: vec![
                    Features::RuntimeBenchmarks,
                    Features::TryRuntime,
                    Features::Std,
                ],
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
