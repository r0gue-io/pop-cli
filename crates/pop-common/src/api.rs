// SPDX-License-Identifier: GPL-3.0

use crate::APP_USER_AGENT;
use bytes::Bytes;
use reqwest::IntoUrl;
use serde::de::DeserializeOwned;
#[cfg(test)]
use std::collections::HashMap;
use std::{
	error::Error as _,
	ops::Deref,
	sync::{Arc, Mutex},
	time::{SystemTime, SystemTimeError},
};
use thiserror::Error;
use tokio::{
	sync::{AcquireError, Semaphore},
	time::{Duration, Instant},
};

/// An API client.
pub(crate) struct ApiClient {
	permits: Arc<Semaphore>,
	token: Option<String>,
	rate_limits: Arc<Mutex<RateLimits>>,
	#[cfg(test)]
	cache: Arc<Mutex<HashMap<String, ApiResponse>>>,
}
impl ApiClient {
	/// A new API Client.
	///
	/// # Arguments
	/// * `max_concurrent` - The maximum number of concurrent requests.
	/// * `token` - An optional API token. If provided, the client will include an `Authorization`
	///   header with the value `token <token>`
	pub(crate) fn new(max_concurrent: usize, token: Option<String>) -> Self {
		Self {
			permits: Arc::new(Semaphore::new(max_concurrent)),
			token,
			rate_limits: Arc::new(Mutex::new(RateLimits::default())),
			#[cfg(test)]
			cache: Arc::new(Mutex::new(HashMap::new())),
		}
	}

	/// Sends a GET request to the provided URL.
	///
	/// # Arguments
	/// * `url` - The URL of the API endpoint to request.
	pub(crate) async fn get(&self, url: impl IntoUrl) -> Result<ApiResponse, Error> {
		let url = url.into_url()?;

		#[cfg(test)]
		// Check if a request for url already cached
		if let Some(response) =
			&self.cache.lock().map_err(|_| Error::LockAcquisitionError)?.get(url.as_str())
		{
			return Ok((*response).clone())
		}

		// Acquire a permit based on the concurrency control
		let _permit = self.permits.acquire().await?;

		// Check if prior evidence of being rate limited
		// Note: only applies if multiple attempts within the same process (e.g., tests)
		let mut rate_limits = self.rate_limits.lock().map_err(|_| Error::LockAcquisitionError)?;
		if let Some(0) = rate_limits.remaining {
			if let Some(reset) = rate_limits.reset {
				let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
				if now < reset {
					return Err(rate_limits.deref().into());
				}
			}
		}

		// Build request, adding any token if present
		let client = reqwest::Client::builder().user_agent(APP_USER_AGENT).build()?;
		let mut request = client.get(url.clone());
		if let Some(token) = &self.token {
			request = request.header("Authorization", format!("token {}", token));
		}

		// Send request, updating rate limits from response headers
		let response = request.send().await?;
		let headers = response.headers();
		rate_limits.limit = headers
			.get("x-ratelimit-limit")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.parse::<u64>().ok());
		rate_limits.remaining = headers
			.get("x-ratelimit-remaining")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.parse::<u64>().ok());
		rate_limits.reset = headers
			.get("x-ratelimit-reset")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.parse::<u64>().ok());
		rate_limits.retry_after = headers
			.get("retry-after")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.parse::<u64>().ok())
			.map(|v| Instant::now() + Duration::from_secs(v));

		// Check if the response indicates rate limiting
		if let Some(0) = rate_limits.remaining {
			return Err(rate_limits.deref().into());
		}

		match response.error_for_status() {
			Ok(response) => {
				let response = ApiResponse(response.bytes().await?);

				// Cache response for any later requests for the same url
				#[cfg(test)]
				self.cache
					.lock()
					.map_err(|_| Error::LockAcquisitionError)?
					.insert(url.to_string(), response.clone());

				Ok(response)
			},
			Err(e) => Err(e.into()),
		}
	}
}

/// An API response.
#[derive(Clone)]
pub(crate) struct ApiResponse(Bytes);

impl ApiResponse {
	/// Attempts to deserialize the API response as JSON.
	pub(crate) async fn json<T: DeserializeOwned>(&self) -> Result<T, Error> {
		serde_json::from_slice(&self.0).map_err(|e| e.into())
	}
}

impl AsRef<[u8]> for ApiResponse {
	fn as_ref(&self) -> &[u8] {
		self.0.as_ref()
	}
}

impl Deref for ApiResponse {
	type Target = [u8];

	#[inline]
	fn deref(&self) -> &[u8] {
		&self.0.deref()
	}
}

#[derive(Debug, Default)]
struct RateLimits {
	limit: Option<u64>,
	remaining: Option<u64>,
	reset: Option<u64>,
	retry_after: Option<Instant>,
}

impl From<&RateLimits> for Error {
	fn from(v: &RateLimits) -> Self {
		Error::RateLimited {
			limit: v.limit,
			remaining: v.remaining,
			reset: v.reset,
			retry_after: v.retry_after,
		}
	}
}

/// An error returned by the API client.
#[derive(Error, Debug)]
pub enum Error {
	/// A decoding error occurred.
	#[error("Decoding error: {0}")]
	DecodeError(#[from] serde_json::Error),
	/// A HTTP error occurred.
	#[error("HTTP error: {0} caused by {:?}", reqwest::Error::source(.0))]
	HttpError(#[from] reqwest::Error),
	/// An error occurred acquiring a lock.
	#[error("Lock acquisition error")]
	LockAcquisitionError,
	/// An API call failed due to rate limiting.
	#[error("Rate limited: limit {limit:?}, remaining {remaining:?}, reset {reset:?}, retry after {retry_after:?}")]
	RateLimited {
		/// If present, the maximum number of requests allowed in the current time window.
		limit: Option<u64>,
		/// If present, the number of requests remaining in the current time window.
		remaining: Option<u64>,
		/// If present, the time (in UTC epoch seconds) at which the rate limit will reset.
		reset: Option<u64>,
		/// If present, the time at which the rate limit will reset.
		retry_after: Option<Instant>,
	},
	/// An error occurred while attempting to convert a time.
	#[error("Time error: {0}")]
	TimeError(#[from] SystemTimeError),
	/// A synchronization error occurred.
	#[error("Synchronization error: {0}")]
	SynchronizationError(#[from] AcquireError),
}
