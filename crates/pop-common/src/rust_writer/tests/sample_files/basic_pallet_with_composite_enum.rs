#![cfg_attr(not(feature = "std"), no_std)]

use frame::prelude::*;

use frame::traits::{fungible, VariantCount};

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

    #[pallet::composite_enum]
    pub enum SomeEnum {
        #[codec(index = 0)]
        Something,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {}
}
