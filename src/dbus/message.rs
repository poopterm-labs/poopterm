//! D-Bus messages: the header with its fields, and the body.
use super::wire::{Invalid, MAX_MESSAGE, Reader, Value, decode_values, encode_values, pad};

type Result<T> = std::result::Result<T, Invalid>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
	MethodCall = 1,
	Return = 2,
	Error = 3,
	Signal = 4,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Message {
	pub kind: Kind,
	pub flags: u8,
	pub serial: u32,
	pub path: Option<String>,
	pub interface: Option<String>,
	pub member: Option<String>,
	pub error_name: Option<String>,
	pub reply_serial: Option<u32>,
	pub destination: Option<String>,
	pub sender: Option<String>,
	pub body: Vec<Value>,
}

impl Message {
	pub(crate) fn method_call(destination: &str, path: &str, interface: &str, member: &str) -> Self {
		Self {
			kind: Kind::MethodCall,
			flags: 0,
			serial: 0,
			path: Some(path.into()),
			interface: Some(interface.into()),
			member: Some(member.into()),
			error_name: None,
			reply_serial: None,
			destination: Some(destination.into()),
			sender: None,
			body: Vec::new(),
		}
	}

	pub(crate) fn encode(&self) -> Vec<u8> {
		let body = encode_values(&self.body);
		let signature: String = self.body.iter().map(Value::signature).collect();
		let text = |code: u8, value: &Option<String>, wrap: fn(String) -> Value| {
			value.clone().map(|value| {
				Value::Struct(vec![
					Value::Byte(code),
					Value::Variant(Box::new(wrap(value))),
				])
			})
		};
		let mut fields: Vec<Value> = [
			text(1, &self.path, Value::Path),
			text(2, &self.interface, Value::Str),
			text(3, &self.member, Value::Str),
			text(4, &self.error_name, Value::Str),
			self.reply_serial.map(|serial| {
				Value::Struct(vec![
					Value::Byte(5),
					Value::Variant(Box::new(Value::U32(serial))),
				])
			}),
			text(6, &self.destination, Value::Str),
			text(7, &self.sender, Value::Str),
		]
		.into_iter()
		.flatten()
		.collect();
		if !signature.is_empty() {
			fields.extend(text(8, &Some(signature), Value::Signature));
		}
		let mut out = encode_values(&[
			Value::Byte(b'l'),
			Value::Byte(self.kind as u8),
			Value::Byte(self.flags),
			Value::Byte(1),
			Value::U32(body.len() as u32),
			Value::U32(self.serial),
			Value::Array("(yv)".into(), fields),
		]);
		pad(&mut out, 8);
		out.extend_from_slice(&body);
		out
	}

	/// Reads one whole message, as long as [`frame_len`] says.
	pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
		if frame_len(bytes.get(..16).ok_or(Invalid)?) != Some(bytes.len()) {
			return Err(Invalid);
		}
		let big_endian = bytes[0] == b'B';
		let kind = match bytes[1] {
			1 => Kind::MethodCall,
			2 => Kind::Return,
			3 => Kind::Error,
			4 => Kind::Signal,
			_ => return Err(Invalid),
		};
		let mut reader = Reader {
			data: bytes,
			at: 4,
			big_endian,
		};
		let body_length = reader.u32()? as usize;
		let serial = reader.u32()?;
		let Value::Array(_, fields) = reader.value(b"a(yv)", 0)? else {
			return Err(Invalid);
		};
		reader.align(8)?;
		let mut message = Self {
			kind,
			flags: bytes[2],
			serial,
			path: None,
			interface: None,
			member: None,
			error_name: None,
			reply_serial: None,
			destination: None,
			sender: None,
			body: Vec::new(),
		};
		let mut signature = String::new();
		for field in fields {
			let Value::Struct(parts) = field else {
				return Err(Invalid);
			};
			let [Value::Byte(code), Value::Variant(value)] = &parts[..] else {
				return Err(Invalid);
			};
			let text = || value.as_str().map(str::to_owned).ok_or(Invalid);
			match code {
				1 => message.path = Some(text()?),
				2 => message.interface = Some(text()?),
				3 => message.member = Some(text()?),
				4 => message.error_name = Some(text()?),
				5 => message.reply_serial = Some(value.as_u32().ok_or(Invalid)?),
				6 => message.destination = Some(text()?),
				7 => message.sender = Some(text()?),
				8 => signature = text()?,
				// Unknown fields are ignored, as the specification asks.
				_ => {}
			}
		}
		let required = match kind {
			Kind::MethodCall => message.path.is_some() && message.member.is_some(),
			Kind::Signal => {
				message.path.is_some() && message.interface.is_some() && message.member.is_some()
			}
			Kind::Return => message.reply_serial.is_some(),
			Kind::Error => message.reply_serial.is_some() && message.error_name.is_some(),
		};
		if serial == 0 || !required || reader.at + body_length != bytes.len() {
			return Err(Invalid);
		}
		message.body = decode_values(&signature, &bytes[reader.at..], big_endian)?;
		Ok(message)
	}
}

/// The length of the message whose first 16 bytes are `header`, or `None`
/// if they are not a D-Bus header or it would exceed [`MAX_MESSAGE`].
pub(crate) fn frame_len(header: &[u8]) -> Option<usize> {
	let header: &[u8; 16] = header.try_into().ok()?;
	let big_endian = match header[0] {
		b'l' => false,
		b'B' => true,
		_ => return None,
	};
	if header[3] != 1 {
		return None;
	}
	let number = |at: usize| {
		let bytes = header[at..at + 4].try_into().unwrap();
		(if big_endian {
			u32::from_be_bytes(bytes)
		} else {
			u32::from_le_bytes(bytes)
		}) as usize
	};
	let (body, fields) = (number(4), number(12));
	if body > MAX_MESSAGE || fields > MAX_MESSAGE {
		return None;
	}
	let length = (16 + fields).next_multiple_of(8) + body;
	(length <= MAX_MESSAGE).then_some(length)
}
