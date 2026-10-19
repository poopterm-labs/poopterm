//! XTMODKEYS, XTFMTKEYS and their queries, as programs send them.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 5, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn replies(term: &mut Term) -> Vec<String> {
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn xtmodkeys_turns_modify_other_keys_on_and_off() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;2m");
	assert_eq!(t.key_options.modify[4], 2);
	send(&mut t, "\x1b[>4m");
	assert_eq!(
		t.key_options.modify[4], 0,
		"no value restores the initial one"
	);
}

#[test]
fn xtmodkeys_without_parameters_resets_every_resource() {
	let mut t = terminal();
	send(&mut t, "\x1b[>1;3m\x1b[>4;2m\x1b[>m");
	assert_eq!(t.key_options.modify, [0, 2, 2, 0, 0, 0, 0, 0]);
}

#[test]
fn xtmodkeys_reads_the_mask_subparameter() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4:1;2m");
	assert_eq!((t.key_options.modify[4], t.key_options.other_mask), (2, 1));
}

#[test]
fn xtmodkeys_is_not_sgr() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;2mA");
	let cell = t.grid[0][0];
	assert_eq!(cell.attrs, 0, "4 is not underline here");
	assert_eq!(cell.fg, t.default_fg);
}

#[test]
fn xtqmodkeys_answers_with_the_restoring_sequence() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;2m\x1b[?4m\x1b[?1m\x1b[?5m\x1b[?m");
	let empty = "\x1b[>0;0m";
	assert_eq!(
		replies(&mut t),
		["\x1b[>4;2m", "\x1b[>1;2m", empty],
		"none asks for 0"
	);
}

#[test]
fn disabling_a_resource_reports_no_value() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;1m\x1b[>4n\x1b[>n\x1b[?4m\x1b[?2m");
	assert_eq!(t.key_options.modify[4], -1);
	assert_eq!(t.key_options.modify[2], -1, "modifyFunctionKeys by default");
	assert_eq!(replies(&mut t), ["\x1b[>4m", "\x1b[>2m"]);
}

#[test]
fn xtfmtkeys_sets_formats_and_xtqfmtkeys_reports_them() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;1f\x1b[?4g\x1b[>4f\x1b[?4g");
	assert_eq!(replies(&mut t), ["\x1b[>4;1f", "\x1b[>4;0f"]);
}

#[test]
fn xtfmtkeys_without_parameters_resets_every_format() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;1f\x1b[>1;1f\x1b[>f");
	assert_eq!(t.key_options.format, [0; 8]);
}

#[test]
fn format_queries_leave_tab_stops_alone() {
	let mut t = terminal();
	send(&mut t, "\x1b[1;9H\x1b[?0g\x1b[?3g");
	assert!(t.tab_stops[8], "CSI ? Pp g is not TBC");
	assert_eq!(replies(&mut t), ["\x1b[>0;0f", "\x1b[>3;0f"]);
}

#[test]
fn hard_reset_restores_the_initial_options() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;2m\x1b[>4;1f\x1b[>1n\x1bc");
	assert_eq!(t.key_options, Default::default());
}

#[test]
fn key_options_do_not_move_the_cursor_or_cancel_wrap() {
	let mut t = terminal();
	send(&mut t, "\x1b[1;20HX\x1b[>4;2m\x1b[>4n\x1b[>4;1f\x1b[?4m");
	assert_eq!((t.cx, t.cy), (19, 0));
	assert!(t.wrap_pending);
}

#[test]
fn an_explicit_zero_resets_like_no_parameters() {
	// The tokenizer cannot tell `CSI > 0 m` from `CSI > m`; resetting every
	// resource also restores resource 0, so both readings agree.
	let mut t = terminal();
	send(&mut t, "\x1b[>1;3m\x1b[>4;2m\x1b[>0m");
	assert_eq!(t.key_options.modify, [0, 2, 2, 0, 0, 0, 0, 0]);
	send(&mut t, "\x1b[>4;1f\x1b[>0f");
	assert_eq!(t.key_options.format, [0; 8]);
}

#[test]
fn resource_zero_with_a_value_is_set() {
	let mut t = terminal();
	send(&mut t, "\x1b[>0;1m\x1b[>0;1f");
	assert_eq!((t.key_options.modify[0], t.key_options.format[0]), (1, 1));
	send(&mut t, "\x1b[?0m\x1b[?0g");
	assert_eq!(replies(&mut t), ["\x1b[>0;1m", "\x1b[>0;1f"]);
}

#[test]
fn extra_parameters_are_ignored() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4;2;9m\x1b[>4;1;7f\x1b[?4;1m");
	assert_eq!((t.key_options.modify[4], t.key_options.format[4]), (2, 1));
	assert_eq!(
		replies(&mut t),
		["\x1b[>4;2m"],
		"only the first resource is asked for"
	);
}

#[test]
fn the_mask_subparameter_keeps_the_four_modifier_bits() {
	let mut t = terminal();
	send(&mut t, "\x1b[>4:5;2m");
	assert_eq!(t.key_options.other_mask, 5);
	send(&mut t, "\x1b[>4:31;2m");
	assert_eq!(t.key_options.other_mask, 15);
	send(&mut t, "\x1b[>4;2m");
	assert_eq!(t.key_options.other_mask, 0, "no mask clears it");
}

#[test]
fn disabling_a_named_resource_leaves_the_others() {
	let mut t = terminal();
	send(&mut t, "\x1b[>1n\x1b[>3n\x1b[>5n");
	assert_eq!(t.key_options.modify, [0, -1, 2, -1, 0, 0, 0, 0]);
	send(&mut t, "\x1b[>1;2m");
	assert_eq!(t.key_options.modify[1], 2, "XTMODKEYS enables it again");
}

#[test]
fn replies_restore_the_options_they_describe() {
	// A disabled resource answers without a value, which reads back as its
	// initial value, so the round trip covers enabled ones.
	let mut t = terminal();
	send(&mut t, "\x1b[>1;3m\x1b[>2;0m\x1b[>4;2m\x1b[>4;1f");
	for resource in [0, 1, 2, 3, 4, 6, 7] {
		send(&mut t, &format!("\x1b[?{resource}m\x1b[?{resource}g"));
	}
	let restoring = replies(&mut t).concat();
	let mut fresh = terminal();
	send(&mut fresh, &restoring);
	assert_eq!(fresh.key_options.modify, t.key_options.modify);
	assert_eq!(fresh.key_options.format, t.key_options.format);
}

#[test]
fn without_the_intermediate_m_is_still_sgr() {
	let mut t = terminal();
	send(&mut t, "\x1b[4;2mA");
	assert_eq!(t.key_options, Default::default());
	assert_ne!(t.grid[0][0].attrs, 0, "underline and faint");
}

#[test]
fn reserved_and_unknown_resources_are_ignored() {
	let mut t = terminal();
	send(&mut t, "\x1b[>5;2m\x1b[>8;2m\x1b[>5;1f\x1b[>9;1f\x1b[>8n");
	assert_eq!(t.key_options, Default::default());
	send(&mut t, "\x1b[?5m\x1b[?8m\x1b[?5g\x1b[?9g");
	assert!(replies(&mut t).is_empty());
}
