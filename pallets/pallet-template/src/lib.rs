//! # pallet_template pallet
//! Please, document your pallet properly
//! Learn more about everything related to Polkadot SDK development at <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/index.html>. Everythin's there!
//! - [`Config`]
//! - [`Call`]
#![cfg_attr(not(feature = "std"), no_std)]

use frame::prelude::*;

use pallet::*;

// A module where the main logic of the pallet is stored
mod pallet_logic;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The aggregated origin type of the runtime.
		type RuntimeOrigin: From<OriginFor<Self>>
			+ From<Origin<Self>>
			+ Into<Result<Origin<Self>, RuntimeLocalOrigin<Self>>>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// SomethingWentWrong
		SomethingWentWrong,
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub enum Origin<T: Config> {
		Nn,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			todo!("Maybe ensure something about your config types...");
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}
