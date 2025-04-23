// SPDX-License-Identifier: GPL-3.0

use crate::APP_USER_AGENT;
use reqwest::{IntoUrl, Response};
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

pub(crate) struct ApiClient {
	permits: Arc<Semaphore>,
	token: Option<String>,
	rate_limits: Arc<Mutex<RateLimits>>,
}
impl ApiClient {
	pub(crate) fn new(max_concurrent: usize, token: Option<String>) -> Self {
		Self {
			permits: Arc::new(Semaphore::new(max_concurrent)),
			token,
			rate_limits: Arc::new(Mutex::new(RateLimits::default())),
		}
	}

	pub(crate) async fn get<U: IntoUrl>(&self, url: U) -> Result<Response, Error> {
		let _permit = self.permits.acquire().await?;
		let mut rate_limits = self.rate_limits.lock().map_err(|_| Error::LockAcquisitionError)?;

		// Check if prior evidence of being rate limited
		// Note: only applies if multiple attempts within the same process (e.g., tests)
		if let Some(0) = rate_limits.remaining {
			if let Some(reset) = rate_limits.reset {
				let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
				if now < reset {
					return Err(rate_limits.deref().into());
				}
			}
		}

		// Build request
		let client = reqwest::Client::builder().user_agent(APP_USER_AGENT).build()?;
		let mut request = client.get(url);
		if let Some(token) = &self.token {
			request = request.header("Authorization", format!("token {}", token));
		}

		// Send request
		let response = request.send().await?;

		// Update rate limits from response headers
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
		match rate_limits.remaining {
			Some(0) => Err(rate_limits.deref().into()),
			_ => response.error_for_status().map_err(Error::HttpError),
		}
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

#[derive(Error, Debug)]
pub enum Error {
	/// A HTTP error occurred.
	#[error("HTTP error: {0} caused by {:?}", reqwest::Error::source(.0))]
	HttpError(#[from] reqwest::Error),
	/// An error occurred acquiring a lock.
	#[error("Lock acquisition error")]
	LockAcquisitionError,
	/// An API call failed due to rate limiting.
	#[error("Rate limited: limit {limit:?}, remaining {remaining:?}, reset {reset:?}, retry after {retry_after:?}")]
	RateLimited {
		limit: Option<u64>,
		remaining: Option<u64>,
		reset: Option<u64>,
		retry_after: Option<Instant>,
	},
	#[error("Time error: {0}")]
	TimeError(#[from] SystemTimeError),
	#[error("Synchronization error: {0}")]
	SynchronizationError(#[from] AcquireError),
}
