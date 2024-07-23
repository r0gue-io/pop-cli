//! # pallet_template pallet
//! Please, document your pallet properly
//! Learn more about everything related to Polkadot SDK development at https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/index.html
//! - [`Config]
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

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// A helful struct. You can use this kind of struct as a value in the pallet's storage. It helps storing related information together. It's possible to store a SCALE compact number using this kind of struct as well :)
	#[derive(TypeInfo, Encode, Decode, MaxEncodedLen, PartialEq, RuntimeDebugNoBound)]
	#[scale_info(skip_type_params(T))]
	pub struct SomeHelpfulStruct<T: Config> {
		pub account_id: T::AccountId,
		#[codec(compact)]
		pub number: u32,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Someone did something
		SomeoneDidSomething { who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// SomethingWentWrong
		SomethingWentWrong,
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
