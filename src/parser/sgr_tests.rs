//! SGR group handling: colors in both the `;` and the `:` forms, truncated
//! and clamped arguments, and attributes that follow a color.
use super::params::ParamList;
use crate::{
	parser::Parser,
	term::{Term, attr::*},
	theme::{Theme, ThemeKind},
};

fn styled(sgr: &str) -> Term {
	let mut term = Term::new(10, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, format!("\x1b[{sgr}m").as_bytes());
	term
}

#[test]
fn indexed_and_direct_colors_take_both_separators() {
	let term = styled("38;5;10");
	assert_eq!((term.fg, term.fg_id), (term.indexed_color(10), 10));
	assert_eq!(styled("38:5:10").fg, term.fg);
	assert_eq!(styled("48;2;1;2;3").bg, 0x010203);
	assert_eq!(styled("48:2:1:2:3").bg, 0x010203);
	// The colon form may name a color space first, left empty here.
	assert_eq!(styled("48:2::1:2:3").bg, 0x010203);
	assert_eq!(styled("58;2;4;5;6").underline_color, Some(0x040506));
	assert_eq!(
		styled("58:5:1").underline_color,
		Some(term.indexed_color(1))
	);
}

#[test]
fn arguments_beyond_255_are_clamped() {
	let term = styled("38;5;300");
	assert_eq!((term.fg, term.fg_id), (term.indexed_color(255), 255));
	assert_eq!(styled("48;2;256;1000;65535").bg, 0xffffff);
}

#[test]
fn attributes_after_a_color_still_apply() {
	let term = styled("1;38;5;10;4");
	assert!(term.bold);
	assert_eq!(term.fg, term.indexed_color(10));
	assert_eq!(term.attrs & UNDERLINE, UNDERLINE);
	let term = styled("48;2;1;2;3;7");
	assert_eq!(term.bg, 0x010203);
	assert!(term.reverse);
	// Blue (3) is part of the color, not italic.
	assert_eq!(term.attrs & ITALIC, 0);
	// The colon form is one group, so the next group is an attribute.
	let term = styled("38:2::1:2:3;3");
	assert_eq!(term.fg, 0x010203);
	assert_eq!(term.attrs & ITALIC, ITALIC);
}

#[test]
fn truncated_colors_change_nothing_and_swallow_their_arguments() {
	let plain = styled("0");
	for sgr in ["38", "38;5", "48;2;1;2", "58;2", "38;9;1", "38:2:1:2"] {
		let term = styled(sgr);
		assert_eq!((term.fg, term.bg), (plain.fg, plain.bg), "{sgr}");
		assert_eq!(term.underline_color, None, "{sgr}");
	}
	// `38;2;1;2` takes four groups even without the blue one.
	let term = styled("38;2;1;2");
	assert_eq!(term.fg, plain.fg);
	// The `5` after a complete color is blink, not part of the color.
	assert_eq!(styled("38;5;1;5").attrs & BLINK, BLINK);
}

#[test]
fn the_last_of_the_most_parameters_applies() {
	// Thirty-one resets and an underline fill the parameter storage.
	let term = styled(&format!("{}4", "0;".repeat(31)));
	assert_eq!(term.attrs & UNDERLINE, UNDERLINE);
	let term = styled(&format!("{}38;5;9", "0;".repeat(29)));
	assert_eq!(term.fg, term.indexed_color(9));
}

#[test]
fn an_empty_list_resets_the_style() {
	let mut term = styled("1;4;31;42");
	Parser::new().advance(&mut term, b"\x1b[m");
	let plain = styled("0");
	assert_eq!(
		(term.fg, term.bg, term.attrs, term.bold),
		(plain.fg, plain.bg, 0, false)
	);
}

#[test]
fn group_counts_agree_for_every_parameter_store() {
	let flat: &[u16] = &[1, 2, 3];
	assert_eq!(ParamList::group_count(flat), 3);
	let empty: &[u16] = &[];
	assert_eq!(ParamList::group_count(empty), 0);
	let mut params = crate::vt::Params::default();
	params.push_sub(38);
	params.push(2);
	params.push(7);
	assert_eq!(ParamList::group_count(&params), 2);
	assert_eq!(ParamList::get(&params, 1), Some(&[7][..]));
	assert_eq!(ParamList::get(&params, 2), None);
}
