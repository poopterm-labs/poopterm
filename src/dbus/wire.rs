//! The D-Bus wire format of values, written little endian and read in
//! either byte order, refusing anything the specification forbids.

/// Largest message read; notifications need far less than the 128 MiB the
/// specification allows.
pub(super) const MAX_MESSAGE: usize = 1 << 20;
/// Deepest nesting of arrays, structs and variants read.
const MAX_DEPTH: usize = 32;

/// A message or value broke the wire format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Invalid;

type Result<T> = std::result::Result<T, Invalid>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
	Byte(u8),
	Bool(bool),
	I16(i16),
	U16(u16),
	I32(i32),
	U32(u32),
	I64(i64),
	U64(u64),
	Double(f64),
	Str(String),
	Path(String),
	Signature(String),
	/// The element signature and the elements.
	Array(String, Vec<Value>),
	Struct(Vec<Value>),
	DictEntry(Box<Value>, Box<Value>),
	Variant(Box<Value>),
}

impl Value {
	pub(crate) fn signature(&self) -> String {
		match self {
			Self::Byte(_) => "y".into(),
			Self::Bool(_) => "b".into(),
			Self::I16(_) => "n".into(),
			Self::U16(_) => "q".into(),
			Self::I32(_) => "i".into(),
			Self::U32(_) => "u".into(),
			Self::I64(_) => "x".into(),
			Self::U64(_) => "t".into(),
			Self::Double(_) => "d".into(),
			Self::Str(_) => "s".into(),
			Self::Path(_) => "o".into(),
			Self::Signature(_) => "g".into(),
			Self::Array(element, _) => format!("a{element}"),
			Self::Struct(fields) => {
				let fields: String = fields.iter().map(Self::signature).collect();
				format!("({fields})")
			}
			Self::DictEntry(key, value) => format!("{{{}{}}}", key.signature(), value.signature()),
			Self::Variant(_) => "v".into(),
		}
	}

	/// The text of a string, object path or signature.
	pub(crate) fn as_str(&self) -> Option<&str> {
		match self {
			Self::Str(text) | Self::Path(text) | Self::Signature(text) => Some(text),
			_ => None,
		}
	}

	pub(crate) fn as_u32(&self) -> Option<u32> {
		match self {
			Self::U32(value) => Some(*value),
			_ => None,
		}
	}
}

/// The alignment of values whose signature starts with `code`.
fn alignment(code: u8) -> usize {
	match code {
		b'n' | b'q' => 2,
		b'b' | b'i' | b'u' | b's' | b'o' | b'a' | b'h' => 4,
		b'x' | b't' | b'd' | b'(' | b'{' => 8,
		_ => 1,
	}
}

/// The length of the single complete type at the start of `signature`.
fn single_type(signature: &[u8], depth: usize) -> Result<usize> {
	if depth > MAX_DEPTH {
		return Err(Invalid);
	}
	match *signature.first().ok_or(Invalid)? {
		b'y' | b'b' | b'n' | b'q' | b'i' | b'u' | b'x' | b't' | b'd' | b's' | b'o' | b'g' | b'h'
		| b'v' => Ok(1),
		b'a' => {
			let rest = &signature[1..];
			if rest.first() == Some(&b'{') {
				// A dict entry: a basic key and one value, only inside arrays.
				let key = *rest.get(1).ok_or(Invalid)?;
				if !b"ybnqiuxtdsogh".contains(&key) {
					return Err(Invalid);
				}
				let value = single_type(rest.get(2..).ok_or(Invalid)?, depth + 1)?;
				if rest.get(2 + value) != Some(&b'}') {
					return Err(Invalid);
				}
				return Ok(1 + 3 + value);
			}
			Ok(1 + single_type(rest, depth + 1)?)
		}
		b'(' => {
			let mut length = 1;
			while signature.get(length) != Some(&b')') {
				length += single_type(signature.get(length..).ok_or(Invalid)?, depth + 1)?;
			}
			if length == 1 {
				return Err(Invalid);
			}
			Ok(length + 1)
		}
		_ => Err(Invalid),
	}
}

/// Marshals `values` as a message body starting at an 8-byte boundary.
pub(crate) fn encode_values(values: &[Value]) -> Vec<u8> {
	let mut out = Vec::new();
	for value in values {
		write(&mut out, value);
	}
	out
}

pub(super) fn pad(out: &mut Vec<u8>, to: usize) {
	out.resize(out.len().next_multiple_of(to), 0);
}

fn write_str(out: &mut Vec<u8>, text: &str) {
	pad(out, 4);
	out.extend_from_slice(&(text.len() as u32).to_le_bytes());
	out.extend_from_slice(text.as_bytes());
	out.push(0);
}

fn write(out: &mut Vec<u8>, value: &Value) {
	pad(out, alignment(value.signature().as_bytes()[0]));
	match value {
		Value::Byte(byte) => out.push(*byte),
		Value::Bool(flag) => out.extend_from_slice(&u32::from(*flag).to_le_bytes()),
		Value::I16(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::U16(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::I32(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::U32(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::I64(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::U64(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::Double(number) => out.extend_from_slice(&number.to_le_bytes()),
		Value::Str(text) | Value::Path(text) => write_str(out, text),
		Value::Signature(text) => {
			out.push(text.len() as u8);
			out.extend_from_slice(text.as_bytes());
			out.push(0);
		}
		Value::Array(element, items) => {
			let at = out.len();
			out.extend_from_slice(&[0; 4]);
			// The length counts from the first element, after its padding.
			pad(out, alignment(element.as_bytes()[0]));
			let start = out.len();
			for item in items {
				write(out, item);
			}
			let length = (out.len() - start) as u32;
			out[at..at + 4].copy_from_slice(&length.to_le_bytes());
		}
		Value::Struct(fields) => {
			for field in fields {
				write(out, field);
			}
		}
		Value::DictEntry(key, value) => {
			write(out, key);
			write(out, value);
		}
		Value::Variant(inner) => {
			let signature = inner.signature();
			out.push(signature.len() as u8);
			out.extend_from_slice(signature.as_bytes());
			out.push(0);
			write(out, inner);
		}
	}
}

/// Unmarshals `data`, which must hold exactly the values `signature` names.
pub(crate) fn decode_values(signature: &str, data: &[u8], big_endian: bool) -> Result<Vec<Value>> {
	let mut reader = Reader {
		data,
		at: 0,
		big_endian,
	};
	let values = reader.values(signature.as_bytes(), 0)?;
	if reader.at != data.len() {
		return Err(Invalid);
	}
	Ok(values)
}

pub(super) struct Reader<'a> {
	pub data: &'a [u8],
	pub at: usize,
	pub big_endian: bool,
}

impl Reader<'_> {
	fn values(&mut self, mut signature: &[u8], depth: usize) -> Result<Vec<Value>> {
		let mut values = Vec::new();
		while !signature.is_empty() {
			let length = single_type(signature, depth)?;
			values.push(self.value(&signature[..length], depth)?);
			signature = &signature[length..];
		}
		Ok(values)
	}

	fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
		let bytes = self.data.get(self.at..self.at + N).ok_or(Invalid)?;
		self.at += N;
		Ok(bytes.try_into().unwrap())
	}

	/// Skips padding, which must be zero.
	pub(super) fn align(&mut self, to: usize) -> Result<()> {
		let end = self.at.next_multiple_of(to);
		let padding = self.data.get(self.at..end).ok_or(Invalid)?;
		if padding.iter().any(|&byte| byte != 0) {
			return Err(Invalid);
		}
		self.at = end;
		Ok(())
	}

	pub(super) fn u32(&mut self) -> Result<u32> {
		let bytes = self.take()?;
		Ok(if self.big_endian {
			u32::from_be_bytes(bytes)
		} else {
			u32::from_le_bytes(bytes)
		})
	}

	fn number<const N: usize>(&mut self) -> Result<[u8; N]> {
		let mut bytes = self.take::<N>()?;
		if self.big_endian {
			bytes.reverse();
		}
		Ok(bytes)
	}

	/// Text of `length` bytes followed by NUL, without NUL inside.
	fn text(&mut self, length: usize) -> Result<String> {
		let bytes = self
			.data
			.get(self.at..self.at + length + 1)
			.ok_or(Invalid)?;
		self.at += length + 1;
		let (text, nul) = bytes.split_at(length);
		if nul != [0] || text.contains(&0) {
			return Err(Invalid);
		}
		String::from_utf8(text.to_vec()).map_err(|_| Invalid)
	}

	fn signature(&mut self) -> Result<String> {
		let [length] = self.take()?;
		let text = self.text(usize::from(length))?;
		let mut rest = text.as_bytes();
		while !rest.is_empty() {
			rest = &rest[single_type(rest, 0)?..];
		}
		Ok(text)
	}

	/// One value of the single complete type `signature`.
	pub(super) fn value(&mut self, signature: &[u8], depth: usize) -> Result<Value> {
		if depth > MAX_DEPTH {
			return Err(Invalid);
		}
		self.align(alignment(signature[0]))?;
		Ok(match signature[0] {
			b'y' => Value::Byte(self.take::<1>()?[0]),
			b'b' => match self.u32()? {
				0 => Value::Bool(false),
				1 => Value::Bool(true),
				_ => return Err(Invalid),
			},
			b'n' => Value::I16(i16::from_le_bytes(self.number()?)),
			b'q' => Value::U16(u16::from_le_bytes(self.number()?)),
			b'i' => Value::I32(i32::from_le_bytes(self.number()?)),
			b'u' | b'h' => Value::U32(self.u32()?),
			b'x' => Value::I64(i64::from_le_bytes(self.number()?)),
			b't' => Value::U64(u64::from_le_bytes(self.number()?)),
			b'd' => Value::Double(f64::from_le_bytes(self.number()?)),
			b's' => {
				let length = self.u32()? as usize;
				Value::Str(self.text(length)?)
			}
			b'o' => {
				let length = self.u32()? as usize;
				Value::Path(self.text(length)?)
			}
			b'g' => Value::Signature(self.signature()?),
			b'v' => {
				let signature = self.signature()?;
				if signature.is_empty() || single_type(signature.as_bytes(), depth)? != signature.len() {
					return Err(Invalid);
				}
				Value::Variant(Box::new(self.value(signature.as_bytes(), depth + 1)?))
			}
			b'a' => {
				let length = self.u32()? as usize;
				let element = &signature[1..];
				self.align(alignment(element[0]))?;
				let end = self.at.checked_add(length).ok_or(Invalid)?;
				if end > self.data.len() {
					return Err(Invalid);
				}
				let mut items = Vec::new();
				while self.at < end {
					items.push(self.value(element, depth + 1)?);
				}
				if self.at != end {
					return Err(Invalid);
				}
				let element = String::from_utf8(element.to_vec()).map_err(|_| Invalid)?;
				Value::Array(element, items)
			}
			b'(' => Value::Struct(self.values(&signature[1..signature.len() - 1], depth + 1)?),
			b'{' => {
				let mut fields = self.values(&signature[1..signature.len() - 1], depth + 1)?;
				let value = fields.pop().ok_or(Invalid)?;
				let key = fields.pop().ok_or(Invalid)?;
				Value::DictEntry(Box::new(key), Box::new(value))
			}
			_ => return Err(Invalid),
		})
	}
}
