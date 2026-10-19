//! Kitty graphics commands: the `key=value,…` control data before `;`, and
//! the replies sent back.
use std::fmt;

/// Keys are single ASCII letters; values are integers, except for a few
/// keys whose values are single letters (`a`, `t`, `o`, `d`).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Command {
	values: [i64; 52],
	present: u64,
}

impl Default for Command {
	fn default() -> Self {
		Self {
			values: [0; 52],
			present: 0,
		}
	}
}

/// A reply error: an errno-style code and a message (`ENOENT:…`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Error {
	pub code: &'static str,
	pub message: String,
}

impl Error {
	pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
		Self {
			code,
			message: message.into(),
		}
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}:{}", self.code, self.message)
	}
}

fn slot(key: u8) -> Option<usize> {
	match key {
		b'a'..=b'z' => Some(usize::from(key - b'a')),
		b'A'..=b'Z' => Some(26 + usize::from(key - b'A')),
		_ => None,
	}
}

impl Command {
	pub(crate) fn parse(control: &[u8]) -> Result<Self, Error> {
		let mut command = Self::default();
		for pair in control
			.split(|&byte| byte == b',')
			.filter(|pair| !pair.is_empty())
		{
			let invalid = || {
				Error::new(
					"EINVAL",
					format!("bad control data: {}", String::from_utf8_lossy(pair)),
				)
			};
			let [key, b'=', value @ ..] = pair else {
				return Err(invalid());
			};
			let index = slot(*key).ok_or_else(invalid)?;
			let value = if matches!(key, b'a' | b't' | b'o' | b'd') {
				match value {
					[letter] if letter.is_ascii_alphabetic() => i64::from(*letter),
					_ => return Err(invalid()),
				}
			} else {
				std::str::from_utf8(value)
					.ok()
					.and_then(|text| text.parse::<i64>().ok())
					.filter(|number| (i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(number))
					.ok_or_else(invalid)?
			};
			command.values[index] = value;
			command.present |= 1 << index;
		}
		Ok(command)
	}

	pub(crate) fn has(&self, key: u8) -> bool {
		slot(key).is_some_and(|index| self.present & (1 << index) != 0)
	}

	/// The value of a numeric key; 0 when absent.
	pub(crate) fn int(&self, key: u8) -> i64 {
		slot(key).map_or(0, |index| self.values[index])
	}

	/// A non-negative numeric key as `u32`; 0 when absent or negative.
	pub(crate) fn uint(&self, key: u8) -> u32 {
		u32::try_from(self.int(key)).unwrap_or(0)
	}

	/// The value of a letter key, or `default` when absent.
	pub(crate) fn letter(&self, key: u8, default: u8) -> u8 {
		if self.has(key) {
			self.int(key) as u8
		} else {
			default
		}
	}

	pub(crate) fn set(&mut self, key: u8, value: i64) {
		if let Some(index) = slot(key) {
			self.values[index] = value;
			self.present |= 1 << index;
		}
	}

	/// The reply to send for `result`, or `None` when the client gave no id
	/// (`i` or `I`) or asked for quiet (`q=1`: no OK, `q=2`: nothing).
	pub(crate) fn reply(&self, result: &Result<(), Error>) -> Option<Vec<u8>> {
		let quiet = self.int(b'q');
		let (id, number) = (self.uint(b'i'), self.uint(b'I'));
		if id == 0 && number == 0 {
			return None;
		}
		let status = match result {
			Ok(()) if quiet >= 1 => return None,
			Err(_) if quiet >= 2 => return None,
			Ok(()) => "OK".to_owned(),
			Err(error) => error.to_string(),
		};
		let mut keys = Vec::new();
		if id != 0 {
			keys.push(format!("i={id}"));
		}
		if number != 0 {
			keys.push(format!("I={number}"));
		}
		if self.uint(b'p') != 0 {
			keys.push(format!("p={}", self.uint(b'p')));
		}
		Some(format!("\x1b_G{};{status}\x1b\\", keys.join(",")).into_bytes())
	}
}

impl fmt::Debug for Command {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let mut map = f.debug_map();
		for key in (b'a'..=b'z')
			.chain(b'A'..=b'Z')
			.filter(|&key| self.has(key))
		{
			map.entry(&(key as char), &self.int(key));
		}
		map.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_numbers_letters_and_negative_values() {
		let command = Command::parse(b"a=T,f=100,i=4294967295,z=-5,q=2").unwrap();
		assert_eq!(command.letter(b'a', b't'), b'T');
		assert_eq!(command.uint(b'f'), 100);
		assert_eq!(command.uint(b'i'), u32::MAX);
		assert_eq!(command.int(b'z'), -5);
		assert_eq!(command.uint(b'z'), 0);
		assert!(!command.has(b'm'));
		assert_eq!(command.letter(b't', b'd'), b'd');
		assert_eq!(Command::parse(b"").unwrap(), Command::default());
	}

	#[test]
	fn rejects_malformed_control_data() {
		for control in [
			&b"a"[..],
			b"a=",
			b"i=x",
			b"a=TT",
			b"i=99999999999",
			b"=1",
			b"\xff=1",
		] {
			assert_eq!(
				Command::parse(control).map_err(|e| e.code),
				Err("EINVAL"),
				"{control:?}"
			);
		}
	}

	#[test]
	fn replies_name_the_image_and_honor_quiet() {
		let error = Err(Error::new("ENOENT", "no such image"));
		let command = Command::parse(b"i=3,p=7").unwrap();
		assert_eq!(command.reply(&Ok(())).unwrap(), b"\x1b_Gi=3,p=7;OK\x1b\\");
		assert_eq!(
			command.reply(&error).unwrap(),
			b"\x1b_Gi=3,p=7;ENOENT:no such image\x1b\\"
		);
		let numbered = Command::parse(b"i=9,I=2").unwrap();
		assert_eq!(numbered.reply(&Ok(())).unwrap(), b"\x1b_Gi=9,I=2;OK\x1b\\");
		assert_eq!(Command::parse(b"a=t").unwrap().reply(&Ok(())), None);
		assert_eq!(Command::parse(b"i=1,q=1").unwrap().reply(&Ok(())), None);
		assert!(Command::parse(b"i=1,q=1").unwrap().reply(&error).is_some());
		assert_eq!(Command::parse(b"i=1,q=2").unwrap().reply(&error), None);
	}
}
