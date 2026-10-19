//! Poopterm's terminfo capabilities, read from the embedded entry, for
//! XTGETTCAP; termcap names of keys and colors are accepted too.
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Capability {
	Flag,
	Number(u32),
	/// With escapes resolved, as the terminal would send or receive it.
	Text(Vec<u8>),
}

/// The capability called `name` (terminfo or termcap) in poopterm's entry.
pub(crate) fn capability(name: &str) -> Option<Capability> {
	static ENTRY: OnceLock<HashMap<String, Capability>> = OnceLock::new();
	let entry = ENTRY.get_or_init(|| parse(crate::terminfo::SOURCE));
	let name = match name {
		"TN" | "name" => return Some(Capability::Text(crate::terminfo::NAME.into())),
		// ncurses' direct color: 8 bits each of red, green and blue.
		"RGB" => return Some(Capability::Text(b"8/8/8".to_vec())),
		"Co" => "colors",
		_ => TERMCAP_KEYS
			.iter()
			.find(|(termcap, _)| *termcap == name)
			.map_or(name, |(_, terminfo)| terminfo),
	};
	entry.get(name).cloned()
}

/// Termcap names of special keys and their terminfo names.
const TERMCAP_KEYS: [(&str, &str); 23] = [
	("kb", "kbs"),
	("ku", "kcuu1"),
	("kd", "kcud1"),
	("kl", "kcub1"),
	("kr", "kcuf1"),
	("kh", "khome"),
	("@7", "kend"),
	("kN", "knp"),
	("kP", "kpp"),
	("kI", "kich1"),
	("kD", "kdch1"),
	("kB", "kcbt"),
	("k1", "kf1"),
	("k2", "kf2"),
	("k3", "kf3"),
	("k4", "kf4"),
	("k5", "kf5"),
	("k6", "kf6"),
	("k7", "kf7"),
	("k8", "kf8"),
	("k9", "kf9"),
	("k;", "kf10"),
	("F1", "kf11"),
];

/// Reads `name=value`, `name#number` and `name` fields after the names
/// line; comments and cancelled (`name@`) fields are skipped.
fn parse(source: &str) -> HashMap<String, Capability> {
	let text: String = source
		.lines()
		.filter(|line| !line.starts_with('#'))
		.collect::<Vec<_>>()
		.join("\n");
	let mut fields = Vec::new();
	let (mut field, mut escaped) = (String::new(), false);
	for c in text.chars() {
		if c == ',' && !escaped {
			fields.push(std::mem::take(&mut field));
		} else {
			field.push(c);
		}
		escaped = c == '\\' && !escaped;
	}
	// The first field holds the names.
	let fields = fields.iter().skip(1).map(|field| field.trim());
	let mut caps = HashMap::new();
	for field in fields.filter(|field| !field.is_empty() && !field.ends_with('@')) {
		let (name, capability) = if let Some((name, value)) = field.split_once('=') {
			(name, Capability::Text(unescape(value)))
		} else if let Some((name, number)) = field.split_once('#') {
			let Some(number) = parse_number(number) else {
				continue;
			};
			(name, Capability::Number(number))
		} else {
			(field, Capability::Flag)
		};
		caps.insert(name.to_owned(), capability);
	}
	caps
}

/// Decimal, `0x` hexadecimal, or octal with a leading zero.
fn parse_number(text: &str) -> Option<u32> {
	if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
		u32::from_str_radix(hex, 16).ok()
	} else if text.len() > 1
		&& let Some(octal) = text.strip_prefix('0')
	{
		u32::from_str_radix(octal, 8).ok()
	} else {
		text.parse().ok()
	}
}

/// Resolves `\E`, `^X`, `\NNN` and the other terminfo escapes.
fn unescape(value: &str) -> Vec<u8> {
	let mut out = Vec::with_capacity(value.len());
	let mut bytes = value.bytes().peekable();
	while let Some(byte) = bytes.next() {
		match byte {
			b'\\' => {
				let Some(next) = bytes.next() else {
					out.push(b'\\');
					break;
				};
				out.push(match next {
					b'E' | b'e' => 0x1b,
					b'n' | b'l' => b'\n',
					b'r' => b'\r',
					b't' => b'\t',
					b'b' => 0x08,
					b'f' => 0x0c,
					b's' => b' ',
					b'0'..=b'7' => {
						let mut value = u32::from(next - b'0');
						for _ in 0..2 {
							match bytes.peek() {
								Some(&digit @ b'0'..=b'7') => {
									value = value * 8 + u32::from(digit - b'0');
									bytes.next();
								}
								_ => break,
							}
						}
						// NUL is written as 0x80, which strings can hold.
						if value == 0 { 0x80 } else { value as u8 }
					}
					other => other,
				});
			}
			b'^' => match bytes.next() {
				Some(b'?') => out.push(0x7f),
				Some(next) => out.push(next & 0x1f),
				None => out.push(b'^'),
			},
			_ => out.push(byte),
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fields_of_every_kind_are_read() {
		let caps = parse(
			"# comment\nname|alias|Description with blanks,\n\tam, cols#80,\n\
			 \tcolors#0x100, bel=^G, kbs=^?, clear=\\E[H\\E[2J,\n\
			 \tcomma=a\\,b, esc=\\e\\\\\\^\\:\\s\\n\\r\\t\\b\\f\\l, octal=\\072\\0, gone@,\n",
		);
		assert_eq!(caps["am"], Capability::Flag);
		assert_eq!(caps["cols"], Capability::Number(80));
		assert_eq!(caps["colors"], Capability::Number(256));
		assert_eq!(caps["bel"], Capability::Text(b"\x07".to_vec()));
		assert_eq!(caps["kbs"], Capability::Text(b"\x7f".to_vec()));
		assert_eq!(caps["clear"], Capability::Text(b"\x1b[H\x1b[2J".to_vec()));
		assert_eq!(caps["comma"], Capability::Text(b"a,b".to_vec()));
		assert_eq!(
			caps["esc"],
			Capability::Text(b"\x1b\\^: \n\r\t\x08\x0c\n".to_vec())
		);
		// `\0` is NUL written so that strings can hold it.
		assert_eq!(caps["octal"], Capability::Text(b":\x80".to_vec()));
		assert!(!caps.contains_key("gone") && !caps.contains_key("gone@"));
		assert!(!caps.contains_key("name") && !caps.contains_key("Description"));
		assert_eq!(caps.len(), 9);
	}

	#[test]
	fn poopterm_capabilities_are_found_by_either_name() {
		let text = |name| match capability(name) {
			Some(Capability::Text(text)) => String::from_utf8(text).unwrap(),
			other => panic!("{name}: {other:?}"),
		};
		assert_eq!(text("TN"), "xterm-poopterm");
		assert_eq!(text("name"), "xterm-poopterm");
		assert_eq!(text("RGB"), "8/8/8");
		assert_eq!(capability("Co"), Some(Capability::Number(256)));
		assert_eq!(capability("colors"), Some(Capability::Number(256)));
		assert_eq!(text("kf1"), "\x1bOP");
		assert_eq!(text("k1"), "\x1bOP");
		assert_eq!(text("kb"), "\x7f");
		assert_eq!(text("ku"), "\x1bOA");
		assert_eq!(text("Smulx"), "\x1b[4:%p1%dm");
		assert_eq!(capability("am"), Some(Capability::Flag));
		assert_eq!(capability("nope"), None);
		assert_eq!(capability(""), None);
	}
}
