// This file is part of try-runtime-cli.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use sp_version::StateVersion;

use crate::Error;

/// Parse a block hash from a string.
pub fn hash(block_hash: &str) -> Result<String, Error> {
	let (block_hash, offset) = if let Some(block_hash) = block_hash.strip_prefix("0x") {
		(block_hash, 2)
	} else {
		(block_hash, 0)
	};

	if let Some(pos) = block_hash.chars().position(|c| !c.is_ascii_hexdigit()) {
		Err(Error::ParamParsingError(format!(
			"Expected block hash, found illegal hex character at position: {}",
			offset + pos,
		)))
	} else {
		Ok(block_hash.into())
	}
}

/// Parse a URL from a string.
pub fn url(s: &str) -> Result<String, Error> {
	if s.starts_with("ws://") || s.starts_with("wss://") {
		Ok(s.to_string())
	} else {
		Err(Error::ParamParsingError(
			"not a valid WS(S) url: must start with 'ws://' or 'wss://'".to_string(),
		))
	}
}

/// Parse a state version from a string.
pub fn state_version(s: &str) -> Result<StateVersion, Error> {
	s.parse::<u8>()
		.map_err(|_| ())
		.and_then(StateVersion::try_from)
		.map_err(|_| Error::ParamParsingError("Invalid state version.".to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_hash_works() {
		assert!(hash("0x1234567890abcdef").is_ok());
		assert!(hash("1234567890abcdef").is_ok());
		assert!(hash("0x1234567890abcdefg").is_err());
		assert!(hash("1234567890abcdefg").is_err());
	}

	#[test]
	fn parse_url_works() {
		assert!(url("ws://localhost:9944").is_ok());
		assert!(url("wss://localhost:9944").is_ok());
		assert!(url("http://localhost:9944").is_err());
		assert!(url("https://localhost:9944").is_err());
	}

	#[test]
	fn parse_state_version_works() {
		assert!(state_version("0").is_ok());
		assert!(state_version("1").is_ok());
		assert!(state_version("100").is_err());
		assert!(state_version("200").is_err());
	}
}
