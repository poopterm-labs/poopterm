//! Mouse reports for applications that enable xterm mouse tracking.
use super::keys::Modifiers;

/// The button a report refers to, in xterm's numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseButtonCode {
	Left = 0,
	Middle = 1,
	Right = 2,
	/// Motion with no button held (any-event tracking, `?1003`).
	None = 3,
	WheelUp = 64,
	WheelDown = 65,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseAction {
	Press,
	Release,
	Motion,
}

/// One mouse event at a 1-based cell position.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MouseReport {
	pub button: MouseButtonCode,
	pub kind: MouseAction,
	pub col: usize,
	pub row: usize,
	pub mods: Modifiers,
}

/// Encodes a report in SGR (`?1006`) or legacy X10 form. Legacy reports can
/// only address cells up to 223, so farther events return `None`.
pub(crate) fn encode_mouse(report: MouseReport, sgr: bool) -> Option<Vec<u8>> {
	let mods = report.mods;
	let mut code = report.button as usize
		+ 4 * usize::from(mods.shift)
		+ 8 * usize::from(mods.alt)
		+ 16 * usize::from(mods.ctrl);
	if report.kind == MouseAction::Motion {
		code += 32;
	}
	let release = report.kind == MouseAction::Release;
	if sgr {
		let suffix = if release { 'm' } else { 'M' };
		return Some(format!("\x1b[<{code};{};{}{suffix}", report.col, report.row).into_bytes());
	}
	if release {
		// X10 cannot say which button was released.
		code = (code & !3) | 3;
	}
	if report.col > 223 || report.row > 223 {
		return None;
	}
	Some(vec![
		0x1b,
		b'[',
		b'M',
		(code + 32) as u8,
		(report.col + 32) as u8,
		(report.row + 32) as u8,
	])
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::input::keys::Modifiers;

	fn report(button: MouseButtonCode, kind: MouseAction, sgr: bool) -> Option<Vec<u8>> {
		encode_mouse(
			MouseReport {
				button,
				kind,
				col: 1,
				row: 2,
				mods: Modifiers::default(),
			},
			sgr,
		)
	}

	#[test]
	fn legacy_reports_use_raw_offset_bytes_not_decimal_digits() {
		assert_eq!(
			report(MouseButtonCode::Left, MouseAction::Press, false).unwrap(),
			b"\x1b[M\x20\x21\x22"
		);
		assert_eq!(
			report(MouseButtonCode::Left, MouseAction::Release, false).unwrap(),
			b"\x1b[M\x23\x21\x22",
			"X10 releases report button 3"
		);
	}

	#[test]
	fn sgr_reports_keep_the_button_on_release() {
		assert_eq!(
			report(MouseButtonCode::Right, MouseAction::Press, true).unwrap(),
			b"\x1b[<2;1;2M"
		);
		assert_eq!(
			report(MouseButtonCode::Right, MouseAction::Release, true).unwrap(),
			b"\x1b[<2;1;2m"
		);
	}

	#[test]
	fn motion_reports_the_held_button_or_none() {
		assert_eq!(
			report(MouseButtonCode::Middle, MouseAction::Motion, true).unwrap(),
			b"\x1b[<33;1;2M"
		);
		assert_eq!(
			report(MouseButtonCode::None, MouseAction::Motion, true).unwrap(),
			b"\x1b[<35;1;2M"
		);
	}

	#[test]
	fn wheel_and_modifiers_set_their_bits() {
		let mods = Modifiers {
			shift: true,
			ctrl: true,
			..Default::default()
		};
		let bytes = encode_mouse(
			MouseReport {
				button: MouseButtonCode::WheelDown,
				kind: MouseAction::Press,
				col: 3,
				row: 4,
				mods,
			},
			true,
		);
		assert_eq!(bytes.unwrap(), b"\x1b[<85;3;4M");
	}

	#[test]
	fn legacy_reports_cannot_address_far_cells() {
		let far = MouseReport {
			button: MouseButtonCode::Left,
			kind: MouseAction::Press,
			col: 224,
			row: 1,
			mods: Modifiers::default(),
		};
		assert_eq!(encode_mouse(far, false), None);
		assert!(encode_mouse(far, true).is_some());
	}
}
