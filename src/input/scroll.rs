//! Mouse-wheel and touchpad scrolling.

/// Turns wheel and touchpad deltas into whole lines, carrying the fraction
/// so slow touchpad gestures still scroll. Positive values scroll up.
#[derive(Default)]
pub(crate) struct ScrollAccumulator {
	remainder: f64,
}

impl ScrollAccumulator {
	/// Adds a pixel delta, given the height of one line in pixels.
	pub(crate) fn pixels(&mut self, delta: f64, line_height: f64) -> isize {
		self.lines(delta / line_height.max(1.0))
	}

	/// Adds a delta measured in lines.
	pub(crate) fn lines(&mut self, delta: f64) -> isize {
		if delta.signum() != self.remainder.signum() {
			self.remainder = 0.0;
		}
		self.remainder += delta;
		let whole = self.remainder.trunc();
		self.remainder -= whole;
		whole as isize
	}
}

/// Cursor-key presses that emulate scrolling in alternate-screen programs
/// without mouse tracking (xterm's `alternateScroll`), e.g. `less` or `man`.
pub(crate) fn alternate_scroll_bytes(lines: isize, application_cursor: bool) -> Vec<u8> {
	let letter = if lines > 0 { b'A' } else { b'B' };
	let key: [u8; 3] = if application_cursor {
		[0x1b, b'O', letter]
	} else {
		[0x1b, b'[', letter]
	};
	key.repeat(lines.unsigned_abs())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn small_touchpad_deltas_accumulate_into_lines() {
		let mut scroll = ScrollAccumulator::default();
		assert_eq!(scroll.pixels(10.0, 24.0), 0);
		assert_eq!(scroll.pixels(10.0, 24.0), 0);
		assert_eq!(scroll.pixels(10.0, 24.0), 1);
		assert_eq!(scroll.pixels(-60.0, 24.0), -2);
	}

	#[test]
	fn whole_line_deltas_pass_through() {
		let mut scroll = ScrollAccumulator::default();
		assert_eq!(scroll.lines(3.0), 3);
		assert_eq!(scroll.lines(-1.0), -1);
	}

	#[test]
	fn alternate_screen_scrolling_sends_cursor_keys() {
		assert_eq!(alternate_scroll_bytes(2, false), b"\x1b[A\x1b[A");
		assert_eq!(alternate_scroll_bytes(-1, true), b"\x1bOB");
		assert!(alternate_scroll_bytes(0, false).is_empty());
	}
}
