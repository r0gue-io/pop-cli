
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 45.0.0
//! DATE: 2025-03-03 (Y/M/D)
//! HOSTNAME: `Tins-MacBook-Pro.local`, CPU: `<UNKNOWN>`
//!
//! SHORT-NAME: `block`, LONG-NAME: `BlockExecution`, RUNTIME: `parachain-template-runtime`
//! WARMUPS: `10`, REPEAT: `100`
//! WEIGHT-PATH: ``
//! WEIGHT-METRIC: `Average`, WEIGHT-MUL: `1.0`, WEIGHT-ADD: `0`

// Executed Command:
//   frame-omni-bencher
//   v1
//   benchmark
//   overhead
//   --runtime=./tests/runtimes/base_parachain_benchmark.wasm
//   --genesis-builder=runtime
//   --genesis-builder-preset=development

use sp_core::parameter_types;
use sp_weights::{constants::WEIGHT_REF_TIME_PER_NANOS, Weight};

parameter_types! {
	/// Weight of executing an empty block.
	/// Calculated by multiplying the *Average* with `1.0` and adding `0`.
	///
	/// Stats nanoseconds:
	///   Min, Max: 599_250, 763_083
	///   Average:  658_682
	///   Median:   655_583
	///   Std-Dev:  31186.7
	///
	/// Percentiles nanoseconds:
	///   99th: 741_792
	///   95th: 707_958
	///   75th: 674_500
	pub const BlockExecutionWeight: Weight =
		Weight::from_parts(WEIGHT_REF_TIME_PER_NANOS.saturating_mul(658_682), 2_277);
}

#[cfg(test)]
mod test_weights {
	use sp_weights::constants;

	/// Checks that the weight exists and is sane.
	// NOTE: If this test fails but you are sure that the generated values are fine,
	// you can delete it.
	#[test]
	fn sane() {
		let w = super::BlockExecutionWeight::get();

		// At least 100 µs.
		assert!(
			w.ref_time() >= 100u64 * constants::WEIGHT_REF_TIME_PER_MICROS,
			"Weight should be at least 100 µs."
		);
		// At most 50 ms.
		assert!(
			w.ref_time() <= 50u64 * constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Weight should be at most 50 ms."
		);
	}
}
