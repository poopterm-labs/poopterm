//! Decoding the command lines shells report with OSC 133 C (kitty's
//! `cmdline`, quoted like bash's `printf %q`, and `cmdline_url`).

/// Longest command line kept, in bytes.
pub(crate) const MAX_COMMAND_LINE: usize = 4096;

/// Undoes shell quoting: backslash escapes, `'…'`, `"…"` and ANSI-C
/// `$'…'` strings. `None` for unterminated quotes or overlong results.
pub(crate) fn unquote(text: &str) -> Option<String> {
	let mut out = String::with_capacity(text.len());
	let mut chars = text.chars().peekable();
	while let Some(c) = chars.next() {
		match c {
			'\\' => out.push(chars.next()?),
			'\'' => loop {
				match chars.next()? {
					'\'' => break,
					c => out.push(c),
				}
			},
			'"' => loop {
				match chars.next()? {
					'"' => break,
					'\\' => match chars.next()? {
						c @ ('$' | '`' | '"' | '\\') => out.push(c),
						'\n' => {}
						c => out.extend(['\\', c]),
					},
					c => out.push(c),
				}
			},
			'$' if chars.peek() == Some(&'\'') => {
				chars.next();
				ansi_c(&mut chars, &mut out)?;
			}
			c => out.push(c),
		}
		if out.len() > MAX_COMMAND_LINE {
			return None;
		}
	}
	Some(out)
}

/// The rest of a `$'…'` string, up to and including its closing quote.
fn ansi_c(chars: &mut std::iter::Peekable<std::str::Chars>, out: &mut String) -> Option<()> {
	loop {
		match chars.next()? {
			'\'' => return Some(()),
			'\\' => {
				let escape = chars.next()?;
				let code = match escape {
					'a' => 7,
					'b' => 8,
					'e' | 'E' => 0x1b,
					'f' => 0x0c,
					'n' => b'\n'.into(),
					'r' => b'\r'.into(),
					't' => b'\t'.into(),
					'v' => 0x0b,
					'0'..='7' => {
						let rest = digits(chars, 8, 2);
						escape.to_digit(8)? * 8u32.pow(rest.1) + rest.0
					}
					'x' => number(chars, 16, 2)?,
					'u' => number(chars, 16, 4)?,
					'U' => number(chars, 16, 8)?,
					'c' => u32::from(chars.next()?) & 0x1f,
					other => other.into(),
				};
				out.push(char::from_u32(code)?);
			}
			c => out.push(c),
		}
	}
}

/// Up to `most` digits in `radix`: their value and how many there were.
fn digits(chars: &mut std::iter::Peekable<std::str::Chars>, radix: u32, most: u32) -> (u32, u32) {
	let (mut value, mut count) = (0u32, 0);
	while count < most
		&& let Some(digit) = chars.peek().and_then(|c| c.to_digit(radix))
	{
		chars.next();
		value = value * radix + digit;
		count += 1;
	}
	(value, count)
}

/// At least one and up to `most` digits.
fn number(chars: &mut std::iter::Peekable<std::str::Chars>, radix: u32, most: u32) -> Option<u32> {
	let (value, count) = digits(chars, radix, most);
	(count > 0).then_some(value)
}

/// Undoes `%xx` escapes; the result must be UTF-8 and not overlong.
pub(crate) fn percent_decode(text: &str) -> Option<String> {
	let mut out = Vec::with_capacity(text.len());
	let mut bytes = text.bytes();
	while let Some(byte) = bytes.next() {
		if byte == b'%' {
			let high = char::from(bytes.next()?).to_digit(16)?;
			let low = char::from(bytes.next()?).to_digit(16)?;
			out.push((high * 16 + low) as u8);
		} else {
			out.push(byte);
		}
	}
	if out.len() > MAX_COMMAND_LINE {
		return None;
	}
	String::from_utf8(out).ok()
}
