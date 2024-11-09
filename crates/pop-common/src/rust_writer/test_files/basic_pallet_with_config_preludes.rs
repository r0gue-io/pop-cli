// SPDX-License-Identifier: GPL-3.0

#![cfg_attr(not(feature = "std"), no_std)]

use frame::prelude::*;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame::pallet]
pub mod pallet {
    use super::*;
    
    pub mod config_preludes{
        use super::*;
        use frame::runtime::prelude::derive_impl;
        /// Provides a viable default config that can be used with
        /// [`derive_impl`] to derive a testing pallet config
        /// based on this one.
        pub struct TestDefaultConfig;
        
        #[derive_impl(frame_system::config_preludes::TestDefaultConfig, no_aggregated_types)]
        impl frame::deps::frame_system::DefaultConfig for TestDefaultConfig {}
        
        #[register_default_impl(TestDefaultConfig)]
        impl DefaultConfig for TestDefaultConfig {
        }
        
        /// Default configurations of this pallet in a solochain environment.
        pub struct SolochainDefaultConfig;
        
        #[derive_impl(
            frame_system::config_preludes::SolochainDefaultConfig,
            no_aggregated_types
        )]
        impl frame::deps::frame_system::DefaultConfig for SolochainDefaultConfig {}
        
        #[register_default_impl(SolochainDefaultConfig)]
        impl DefaultConfig for SolochainDefaultConfig {
        }
        
        /// Default configurations of this pallet in a solochain environment.
        pub struct RelayChainDefaultConfig;
        
        #[derive_impl(
            frame_system::config_preludes::RelayChainDefaultConfig,
            no_aggregated_types
        )]
        impl frame::deps::frame_system::DefaultConfig for RelayChainDefaultConfig {}
        
        #[register_default_impl(RelayChainDefaultConfig)]
        impl DefaultConfig for RelayChainDefaultConfig {
        }
        
        /// Default configurations of this pallet in a solochain environment.
        pub struct ParaChainDefaultConfig;
        
        #[derive_impl(
            frame_system::config_preludes::ParaChainDefaultConfig,
            no_aggregated_types
        )]
        impl frame::deps::frame_system::DefaultConfig for ParaChainDefaultConfig {}
        
        #[register_default_impl(ParaChainDefaultConfig)]
        impl DefaultConfig for ParaChainDefaultConfig {
        }
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config(with_default)]
    pub trait Config: frame_system::Config {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {}
}
