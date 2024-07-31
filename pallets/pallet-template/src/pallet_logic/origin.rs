use crate::pallet::*;
use frame::prelude::*;

impl<T: Config> Pallet<T> {
	/// A helpful function your pallet may use to convert a external origin to the Nn variant of your custom origin.
	pub(crate) fn origin_to_Nn(
		origin: OriginFor<T>,
	) -> Result<RuntimeLocalOrigin<T>, DispatchError> {
		// You can use the line below if your variant contains a signed AccountId :)
		// let who = ensure_signed(origin)?;
		Ok(<RuntimeLocalOrigin<T>>::from(<Origin<T>>::Nn))
	}
}

impl<T: Config, OuterOrigin: Into<Result<Origin<T>, OuterOrigin>> + From<Origin<T>>>
	EnsureOrigin<OuterOrigin> for Origin<T>
{
	type Success = todo!();

	// This is a good place to define conditions making your origin valid!
	fn try_origin(outer_origin: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		outer_origin.into().and_then(|origin| match origin {
			Nn => todo!(),
			other => Err(OuterOrigin::from(other)),
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<OuterOrigin, ()> {
		Ok(OuterOrigin::from(Origin::Nn))
	}
}
