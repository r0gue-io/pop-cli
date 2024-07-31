use crate::{
	self as pallet_template,
	frame_system::{mocking::MockBlock, GenesisConfig},
};

use frame::{
	deps::frame_support::{derive_impl, parameter_types, runtime},
	runtime::prelude::*,
	testing_prelude::*,
};

type Block = MockBlock<Test>;

#[runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	#[runtime::pallet_index(2)]
	pub type pallet_template = pallet_template;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl crate::frame_system::Config for Test {
	type Block = Block;
}

impl pallet_template::Config for Test {}

pub(crate) struct StateBuilder {}

impl Default for StateBuilder {
	fn default() -> Self {
		Self {}
	}
}

impl StateBuilder {
	pub(crate) fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = crate::frame_system::GenesisConfig::<Test>::default()
			.build_storage()
			.unwrap()
			.into();

		// Test setup
		ext.execute_with(|| {
			System::set_block_number(1);
		});

		ext.execute_with(test);

		// Test assertions
		ext.execute_with(|| {});
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
