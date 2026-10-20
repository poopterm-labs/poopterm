//! How characters of each width class from the generated width table land
//! in the grid, printed in batches and one character at a time.
use crate::{
	parser::{Parser, PerCharacter},
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize) -> Term {
	Term::new(cols, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

/// The terminal after `text`, through the batched parser and per character.
fn both(cols: usize, text: &str) -> [Term; 2] {
	let mut batched = terminal(cols);
	Parser::new().advance(&mut batched, text.as_bytes());
	let mut single = terminal(cols);
	vte::Parser::new().advance(&mut PerCharacter(&mut single), text.as_bytes());
	[batched, single]
}

/// (character, width, marks) of each cell in row 0, then the cursor column.
fn layout(t: &Term) -> (Vec<(char, u8, Vec<char>)>, usize) {
	let cells = t.grid[0]
		.iter()
		.map(|c| (c.ch, c.width, t.marks(c).to_vec()))
		.collect();
	(cells, t.cx)
}

fn assert_layout(cols: usize, text: &str, cells: &[(char, u8, &[char])], cx: usize) {
	let mut expected: Vec<_> = cells.iter().map(|&(c, w, m)| (c, w, m.to_vec())).collect();
	expected.resize(cols, (' ', 1, vec![]));
	for t in both(cols, text) {
		assert_eq!(layout(&t), (expected.clone(), cx), "{text:?}");
	}
}

#[test]
fn hangul_jamo_join_the_leading_consonant_in_one_wide_cell() {
	let syllable = ('\u{1100}', 2, &['\u{1161}', '\u{11a8}'][..]);
	assert_layout(
		6,
		"\u{1100}\u{1161}\u{11a8}x",
		&[syllable, (' ', 0, &[]), ('x', 1, &[])],
		3,
	);
}

#[test]
fn the_hangul_leading_filler_is_wide_though_default_ignorable() {
	let syllable = ('\u{115f}', 2, &['\u{1161}'][..]);
	assert_layout(
		4,
		"\u{115f}\u{1161}x",
		&[syllable, (' ', 0, &[]), ('x', 1, &[])],
		3,
	);
}

#[test]
fn prepended_concatenation_marks_join_the_previous_cell() {
	let cells = [('a', 1, &['\u{d4e}', '\u{111c2}'][..]), ('b', 1, &[])];
	assert_layout(4, "a\u{d4e}\u{111c2}b", &cells, 2);
}

#[test]
fn invisible_characters_join_the_previous_cell() {
	let marks = ['\u{ad}', '\u{200b}', '\u{2060}'];
	assert_layout(
		4,
		"a\u{ad}\u{200b}\u{2060}b",
		&[('a', 1, &marks), ('b', 1, &[])],
		2,
	);
}

#[test]
fn prepended_number_signs_take_a_cell() {
	assert_layout(
		4,
		"\u{600}12",
		&[('\u{600}', 1, &[]), ('1', 1, &[]), ('2', 1, &[])],
		3,
	);
}

#[test]
fn regional_indicators_take_one_cell_each() {
	let flag = [
		('\u{1f1e9}', 1, &[][..]),
		('\u{1f1ea}', 1, &[]),
		('x', 1, &[]),
	];
	assert_layout(4, "\u{1f1e9}\u{1f1ea}x", &flag, 3);
}

#[test]
fn emoji_presentation_is_wide_and_text_presentation_narrow() {
	let cells = [
		('🙂', 2, &[][..]),
		(' ', 0, &[]),
		('\u{2764}', 1, &[]),
		('x', 1, &[]),
	];
	assert_layout(5, "🙂\u{2764}x", &cells, 4);
}

#[test]
fn supplementary_ideographs_and_their_unassigned_planes_are_wide() {
	let cells = [
		('\u{20000}', 2, &[][..]),
		(' ', 0, &[]),
		('\u{3fffd}', 2, &[]),
		(' ', 0, &[]),
	];
	assert_layout(5, "\u{20000}\u{3fffd}", &cells, 4);
}

#[test]
fn wide_characters_take_one_cell_in_a_one_column_terminal() {
	assert_layout(1, "猫", &[('猫', 1, &[])], 0);
}

#[test]
fn a_wide_character_after_the_last_column_wraps_whole() {
	for t in both(3, "ab猫") {
		assert_eq!(
			(t.grid[0][2].ch, t.grid[1][0].ch, t.grid[1][0].width),
			(' ', '猫', 2)
		);
		assert_eq!((t.cx, t.cy), (2, 1));
	}
}
