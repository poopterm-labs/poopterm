//! Bounded parsing of common complete CSI sequences, with atomic fallback.
use crate::term::Term;

/// Only commit after seeing a supported final byte. Partial, complex, and
/// oversized sequences go to vte from their original ESC, without buffering.
#[inline]
pub(super) fn apply(term: &mut Term, bytes: &[u8]) -> Option<usize> {
	if !bytes.starts_with(b"\x1b[") {
		return None;
	}
	let mut params = [0u16; 32];
	let mut index = 0;
	for (offset, &byte) in bytes.iter().take(96).enumerate().skip(2) {
		match byte {
			b'0'..=b'9' => {
				params[index] = params[index]
					.saturating_mul(10)
					.saturating_add(u16::from(byte - b'0'))
			}
			b';' if index + 1 < params.len() => index += 1,
			b'm' | b'H' | b'K' | b'A'..=b'D' => {
				term
					.pending()
					.apply_csi_dispatch(&params[..=index], &[], false, byte as char);
				return Some(offset + 1);
			}
			_ => return None,
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		term::Term,
		theme::{Theme, ThemeKind},
	};

	#[test]
	fn unsupported_or_incomplete_sequences_leave_the_terminal_untouched() {
		let mut term = Term::new(8, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		for bytes in [
			&b"\x1b[31"[..],
			&b"\x1b[?6h"[..],
			&b"\x1b[38:2::1:2:3m"[..],
			&b"\x1b[1\nA"[..],
			&b"\x1b[3J"[..],
		] {
			let fg = term.fg;
			assert_eq!(apply(&mut term, bytes), None);
			assert_eq!((term.cx, term.cy, term.fg), (0, 0, fg));
		}
	}

	#[test]
	fn complete_common_sequences_return_their_exact_length() {
		let mut term = Term::new(8, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		assert_eq!(apply(&mut term, b"\x1b[2;3Htext"), Some(6));
		assert_eq!((term.cx, term.cy), (2, 1));
		assert_eq!(apply(&mut term, b"\x1b[999999D"), Some(9));
		assert_eq!(term.cx, 0);
	}
}
