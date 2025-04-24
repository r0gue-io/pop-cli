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
use tokio::sync::{AcquireError, Semaphore};

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
			.and_then(|v| v.parse::<u64>().ok());

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
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ApiResponse(Bytes);

impl ApiResponse {
	/// Attempts to deserialize the API response as JSON.
	pub(crate) async fn json<T: DeserializeOwned>(&self) -> Result<T, Error> {
		serde_json::from_slice(&self.0).map_err(|e| e.into())
	}
}

impl Deref for ApiResponse {
	type Target = [u8];

	#[inline]
	fn deref(&self) -> &[u8] {
		self.0.deref()
	}
}

#[derive(Debug, Default, PartialEq)]
struct RateLimits {
	limit: Option<u64>,
	remaining: Option<u64>,
	reset: Option<u64>,
	retry_after: Option<u64>,
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
#[allow(clippy::enum_variant_names)]
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
		/// If present, the number of seconds to wait until retrying the request.
		retry_after: Option<u64>,
	},
	/// An error occurred while attempting to convert a time.
	#[error("Time error: {0}")]
	TimeError(#[from] SystemTimeError),
	/// A synchronization error occurred.
	#[error("Synchronization error: {0}")]
	SynchronizationError(#[from] AcquireError),
}

#[cfg(test)]
mod tests {
	use super::{Error::*, *};
	use mockito::Server;
	use reqwest::StatusCode;
	use std::error::Error;

	const LIMIT: u64 = 60;
	const REMAINING: u64 = 10;
	const RETRY_AFTER: u64 = 60;
	const TOKEN: &str = "<TOKEN>";

	#[tokio::test]
	async fn token_authorization_works() -> Result<(), Box<dyn Error>> {
		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/auth")
			.with_status(StatusCode::OK.as_u16().into())
			.match_header("Authorization", format!("token {TOKEN}").as_str())
			.create_async()
			.await;

		let client = ApiClient::new(1, Some(TOKEN.into()));
		client.get(format!("{}/auth", server.url())).await?;

		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn extracts_rate_limits_from_response_headers() -> Result<(), Box<dyn Error>> {
		let reset =
			SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() + RETRY_AFTER;

		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/rate-limits")
			.with_header("x-ratelimit-limit", LIMIT.to_string().as_str())
			.with_header("x-ratelimit-remaining", REMAINING.to_string().as_str())
			.with_header("x-ratelimit-reset", reset.to_string().as_str())
			.with_header("retry-after", RETRY_AFTER.to_string().as_str())
			.with_status(StatusCode::OK.as_u16().into())
			.create_async()
			.await;

		let client = ApiClient::new(1, Some(TOKEN.into()));
		client.get(format!("{}/rate-limits", server.url())).await?;

		assert_eq!(
			*client.rate_limits.lock().unwrap(),
			RateLimits {
				limit: Some(LIMIT),
				remaining: Some(REMAINING),
				reset: Some(reset),
				retry_after: Some(RETRY_AFTER),
			}
		);

		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn returns_rate_limited_error_when_no_requests_remaining() -> Result<(), Box<dyn Error>> {
		let reset =
			SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() + RETRY_AFTER;

		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/rate-limited")
			.with_header("x-ratelimit-limit", LIMIT.to_string().as_str())
			.with_header("x-ratelimit-remaining", "0")
			.with_header("x-ratelimit-reset", reset.to_string().as_str())
			.with_header("retry-after", RETRY_AFTER.to_string().as_str())
			.with_status(StatusCode::OK.as_u16().into())
			.expect_at_least(1)
			.expect_at_most(1)
			.create_async()
			.await;

		let client = ApiClient::new(1, Some(TOKEN.into()));
		for _ in 0..5 {
			assert!(
				matches!(client.get(format!("{}/rate-limited", server.url())).await, Err(RateLimited {
				limit,
				remaining,
				reset: _reset,
				retry_after
			}) if limit == Some(LIMIT) && remaining == Some(0) && _reset == Some(reset) && retry_after == Some(RETRY_AFTER) )
			);
		}

		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn returns_underlying_error_otherwise() -> Result<(), Box<dyn Error>> {
		const STATUS_CODE: StatusCode = StatusCode::FORBIDDEN;

		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/error")
			.with_status(STATUS_CODE.as_u16().into())
			.create_async()
			.await;

		let client = ApiClient::new(1, None);
		assert!(matches!(
				client.get(format!("{}/error", server.url())).await, 
				Err(HttpError(e)) if e.status() == Some(STATUS_CODE)));

		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn returns_bytes() -> Result<(), Box<dyn Error>> {
		let payload = b"<API_RESPONSE>";

		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/bytes")
			.with_status(StatusCode::OK.as_u16().into())
			.with_body(payload)
			.create_async()
			.await;

		let client = ApiClient::new(1, None);
		let response = client.get(format!("{}/bytes", server.url())).await?;
		assert_eq!(*response, *payload);

		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn returns_json() -> Result<(), Box<dyn Error>> {
		let payload = b"{\"key\": \"value\"}";

		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/json")
			.with_status(StatusCode::OK.as_u16().into())
			.with_body(payload)
			.create_async()
			.await;

		let client = ApiClient::new(1, None);
		let response = client
			.get(format!("{}/json", server.url()))
			.await?
			.json::<serde_json::Value>()
			.await?;
		assert_eq!(response, serde_json::json!({ "key": "value" }));

		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn test_caching_works() -> Result<(), Box<dyn Error>> {
		let payload = b"<API_RESPONSE>";

		let mut server = Server::new_async().await;
		let mock = server
			.mock("GET", "/cache")
			.with_status(StatusCode::OK.as_u16().into())
			.with_body(payload)
			.expect_at_least(1)
			.expect_at_most(1)
			.create_async()
			.await;

		let client = ApiClient::new(1, None);
		let url = format!("{}/cache", server.url());

		for _ in 0..5 {
			let response = client.get(url.clone()).await?;
			assert_eq!(*response, *payload);
			assert_eq!(client.cache.lock().unwrap().get(&url), Some(&response));
		}

		mock.assert_async().await;
		Ok(())
	}
}
