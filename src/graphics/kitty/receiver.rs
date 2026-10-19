//! Receives kitty graphics APC strings (`ESC _ G <control> ; <base64> ESC \`)
//! in pieces, decoding the payload as it arrives and joining chunked
//! transmissions (`m=1`).
use super::{Command, Error, MAX_TRANSFER};
use crate::graphics::base64::Decoder;

/// Longest control part accepted (`key=value,…` before `;`).
pub(crate) const MAX_CONTROL: usize = 1024;

/// A complete command with its decoded payload.
pub(crate) struct Transmission {
	pub command: Command,
	pub data: Result<Vec<u8>, Error>,
}

#[derive(Default)]
enum State {
	#[default]
	Idle,
	/// Waiting for the first byte, which must be `G`.
	Kind,
	Ignore,
	Control,
	Payload,
}

/// A transmission being received, possibly over several chunks.
struct Upload {
	/// The command of the first chunk; later chunks only carry `m`.
	command: Command,
	decoder: Decoder,
	data: Vec<u8>,
	error: Option<Error>,
}

pub(crate) struct Receiver {
	state: State,
	control: Vec<u8>,
	control_overflow: bool,
	upload: Option<Upload>,
	/// The current chunk has `m=1`: more chunks follow.
	more: bool,
	limit: usize,
}

/// A cloned terminal gets an idle receiver, so a transfer is never copied.
impl Clone for Receiver {
	fn clone(&self) -> Self {
		Self::default()
	}
}

impl Default for Receiver {
	fn default() -> Self {
		Self {
			state: State::Idle,
			control: Vec::new(),
			control_overflow: false,
			upload: None,
			more: false,
			limit: MAX_TRANSFER,
		}
	}
}

impl Receiver {
	#[cfg(test)]
	pub(crate) fn with_limit(limit: usize) -> Self {
		Self {
			limit,
			..Self::default()
		}
	}

	pub(crate) fn start(&mut self) {
		self.state = State::Kind;
	}

	pub(crate) fn put(&mut self, mut bytes: &[u8]) {
		loop {
			match self.state {
				State::Idle | State::Ignore => return,
				State::Kind => {
					let Some((&first, rest)) = bytes.split_first() else {
						return;
					};
					if first != b'G' {
						self.state = State::Ignore;
						return;
					}
					self.control.clear();
					self.control_overflow = false;
					self.state = State::Control;
					bytes = rest;
				}
				State::Control => {
					let end = bytes.iter().position(|&byte| byte == b';');
					let part = &bytes[..end.unwrap_or(bytes.len())];
					if self.control.len() + part.len() > MAX_CONTROL {
						self.control_overflow = true;
					} else {
						self.control.extend_from_slice(part);
					}
					let Some(end) = end else {
						return;
					};
					self.begin_payload();
					bytes = &bytes[end + 1..];
				}
				State::Payload => {
					self.decode(bytes);
					return;
				}
			}
		}
	}

	/// Ends the APC string. Returns the transmission once its last chunk
	/// arrived; `complete` is false when the string was cancelled, which
	/// abandons the whole transmission.
	pub(crate) fn end(&mut self, complete: bool) -> Option<Transmission> {
		match std::mem::take(&mut self.state) {
			State::Idle | State::Kind | State::Ignore => return None,
			_ if !complete => {
				self.upload = None;
				return None;
			}
			State::Control => self.begin_payload(),
			State::Payload => {}
		}
		let mut upload = self.upload.take()?;
		if self.more {
			// Clients that encode every chunk separately pad each one.
			if upload.decoder.padded() {
				upload.decoder = Decoder::default();
			}
			self.upload = Some(upload);
			return None;
		}
		if upload.error.is_none() && upload.decoder.finish(&mut upload.data).is_err() {
			upload.error = Some(bad_base64());
		}
		Some(Transmission {
			command: upload.command,
			data: match upload.error {
				Some(error) => Err(error),
				None => Ok(upload.data),
			},
		})
	}

	/// The control part is complete: start or continue a transmission.
	fn begin_payload(&mut self) {
		self.state = State::Payload;
		let command = if self.control_overflow {
			Err(Error::new("EINVAL", "control data too long"))
		} else {
			Command::parse(&self.control)
		};
		match command {
			Ok(command) => {
				self.more = command.int(b'm') == 1;
				// Continuation chunks carry only `m`, `q` and `a=f`; a delete
				// command aborts the transmission in progress.
				if command.letter(b'a', 0) == b'd' {
					self.upload = None;
				}
				if self.upload.is_none() {
					self.upload = Some(Upload {
						command,
						decoder: Decoder::default(),
						data: Vec::new(),
						error: None,
					});
				}
			}
			Err(error) => {
				self.more = false;
				self.upload = Some(Upload {
					command: Command::default(),
					decoder: Decoder::default(),
					data: Vec::new(),
					error: Some(error),
				});
			}
		}
	}

	fn decode(&mut self, bytes: &[u8]) {
		let Some(upload) = &mut self.upload else {
			return;
		};
		if upload.error.is_some() {
			return;
		}
		let result = upload.decoder.push(bytes, &mut upload.data);
		if result.is_err() {
			upload.error = Some(bad_base64());
		} else if upload.data.len() > self.limit {
			upload.error = Some(Error::new("EFBIG", "image data too large"));
		} else {
			return;
		}
		upload.data = Vec::new();
	}
}

fn bad_base64() -> Error {
	Error::new("EINVAL", "payload is not valid base64")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bench::images::base64;

	/// Feeds one APC string in pieces of `step` bytes.
	fn apc(receiver: &mut Receiver, body: &[u8], step: usize) -> Option<Transmission> {
		receiver.start();
		for piece in body.chunks(step.max(1)) {
			receiver.put(piece);
		}
		receiver.end(true)
	}

	#[test]
	fn decodes_control_and_payload_split_anywhere() {
		let mut body = b"Ga=T,f=24,s=1,v=1;".to_vec();
		body.extend(base64(&[1, 2, 3]));
		for step in 1..body.len() {
			let mut receiver = Receiver::default();
			let transmission = apc(&mut receiver, &body, step).unwrap();
			assert_eq!(transmission.command.letter(b'a', 0), b'T');
			assert_eq!(transmission.command.uint(b'f'), 24);
			assert_eq!(transmission.data.unwrap(), [1, 2, 3]);
		}
	}

	#[test]
	fn other_apc_strings_and_cancelled_ones_are_ignored() {
		let mut receiver = Receiver::default();
		assert!(apc(&mut receiver, b"Xa=T;AAAA", 3).is_none());
		receiver.start();
		receiver.put(b"Ga=T;AAAA");
		assert!(receiver.end(false).is_none());
		// Without a payload, the control data alone is the command.
		let query = apc(&mut receiver, b"Ga=d,d=A", 4).unwrap();
		assert_eq!(query.command.letter(b'd', 0), b'A');
		assert_eq!(query.data.unwrap(), b"");
	}

	#[test]
	fn chunks_join_into_one_transmission_with_the_first_command() {
		let data: Vec<u8> = (0..=255).collect();
		let encoded = base64(&data);
		let (first, rest) = encoded.split_at(100);
		let mut receiver = Receiver::default();
		let mut body = b"Ga=t,i=5,f=32,s=8,v=8,m=1;".to_vec();
		body.extend_from_slice(first);
		assert!(apc(&mut receiver, &body, 7).is_none());
		let mut body = b"Gm=1;".to_vec();
		body.extend_from_slice(&rest[..50]);
		assert!(apc(&mut receiver, &body, 7).is_none());
		let mut body = b"Gm=0;".to_vec();
		body.extend_from_slice(&rest[50..]);
		let transmission = apc(&mut receiver, &body, 7).unwrap();
		assert_eq!(transmission.command.uint(b'i'), 5);
		assert_eq!(transmission.data.unwrap(), data);
		// The next command starts fresh.
		let next = apc(&mut receiver, b"Ga=p,i=5", 3).unwrap();
		assert_eq!(next.command.letter(b'a', 0), b'p');
	}

	#[test]
	fn a_cancelled_chunk_abandons_the_whole_transmission() {
		let mut receiver = Receiver::default();
		assert!(apc(&mut receiver, b"Ga=t,i=1,m=1;AAAA", 5).is_none());
		receiver.start();
		receiver.put(b"Gm=0;AAAA");
		assert!(receiver.end(false).is_none());
		let next = apc(&mut receiver, b"Ga=t,i=2;AAAA", 5).unwrap();
		assert_eq!(next.command.uint(b'i'), 2);
		assert_eq!(next.data.unwrap(), [0, 0, 0]);
	}

	#[test]
	fn a_delete_command_aborts_a_chunked_transmission() {
		let mut receiver = Receiver::default();
		assert!(apc(&mut receiver, b"Ga=t,i=1,m=1;AAAA", 5).is_none());
		let delete = apc(&mut receiver, b"Ga=d,d=i,i=1", 5).unwrap();
		assert_eq!(delete.command.letter(b'a', 0), b'd');
		// The rest of the abandoned transmission is not joined to anything.
		let next = apc(&mut receiver, b"Ga=t,i=2;AAAA", 5).unwrap();
		assert_eq!(next.command.uint(b'i'), 2);
	}

	#[test]
	fn bad_control_data_bad_base64_and_oversize_payloads_are_errors() {
		let mut receiver = Receiver::default();
		let bad = apc(&mut receiver, b"Gi=x;AAAA", 2).unwrap();
		assert_eq!(bad.data.unwrap_err().code, "EINVAL");
		let bad = apc(&mut receiver, b"Gi=1;AA!A", 2).unwrap();
		assert_eq!(bad.command.uint(b'i'), 1);
		assert_eq!(bad.data.unwrap_err().code, "EINVAL");
		let mut long = b"G".to_vec();
		long.extend(std::iter::repeat_n(b'i', MAX_CONTROL + 1));
		assert_eq!(
			apc(&mut receiver, &long, 64)
				.unwrap()
				.data
				.unwrap_err()
				.code,
			"EINVAL"
		);

		let mut receiver = Receiver::with_limit(6);
		let big = apc(&mut receiver, b"Gi=1;AAAAAAAAAAAA", 3).unwrap();
		assert_eq!(big.data.unwrap_err().code, "EFBIG");
		let fits = apc(&mut receiver, b"Gi=1;AAAAAAAA", 3).unwrap();
		assert_eq!(fits.data.unwrap().len(), 6);
	}
}
