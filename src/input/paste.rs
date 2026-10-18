//! Preparing clipboard text for the PTY.

const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

/// Converts clipboard text into PTY input. Line endings become CR, as a
/// typed Enter would be. With bracketed paste (`?2004`) the text is wrapped
/// in markers, and any markers inside it are removed so pasted content
/// cannot end the paste early and inject keystrokes.
pub(crate) fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
	let mut text = text.replace("\r\n", "\r").replace('\n', "\r");
	if bracketed {
		while text.contains(PASTE_START) || text.contains(PASTE_END) {
			text = text.replace(PASTE_START, "").replace(PASTE_END, "");
		}
		text = format!("{PASTE_START}{text}{PASTE_END}");
	}
	text.into_bytes()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bracketed_paste_cannot_be_terminated_by_the_pasted_text() {
		let bytes = paste_bytes("ls\x1b[201~rm -rf ~\n", true);
		assert_eq!(bytes, b"\x1b[200~lsrm -rf ~\r\x1b[201~");
	}

	#[test]
	fn newlines_are_sent_as_carriage_returns() {
		assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc");
	}
}
