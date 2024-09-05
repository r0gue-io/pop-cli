// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_transcode::{ink_metadata::MessageParamSpec, ContractMessageTranscoder};
use scale_info::form::PortableForm;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
// TODO: We are ignoring selector, return type for now.
/// Describes a contract message.
pub struct Message {
	/// The label of the message.
	pub label: String,
	/// If the message is allowed to mutate the contract state.
	pub mutates: bool,
	/// If the message accepts any `value` from the caller.
	pub payable: bool,
	/// The parameters of the deployment handler.
	pub args: Vec<String>,
	/// The message documentation.
	pub docs: String,
	/// If the message is the default for off-chain consumers (e.g UIs).
	pub default: bool,
}

pub fn get_messages(metadata_path: PathBuf) -> Result<Vec<Message>, Error> {
	let transcoder = ContractMessageTranscoder::load(metadata_path)?;
	let mut messages: Vec<Message> = Vec::new();
	for message in transcoder.metadata().spec().messages() {
		messages.push(Message {
			label: message.label().to_string(),
			mutates: message.mutates(),
			payable: message.payable(),
			args: process_args(message.args()),
			docs: message.docs().join("."),
			default: *message.default(),
		});
	}
	Ok(messages)
}
//TODO: We are ignoring the type of the argument.
fn process_args(message_params: &[MessageParamSpec<PortableForm>]) -> Vec<String> {
	let mut args: Vec<String> = Vec::new();
	for arg in message_params {
		args.push(arg.label().to_string());
	}
	args
}
