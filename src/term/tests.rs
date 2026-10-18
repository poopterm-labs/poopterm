use crate::{
	parser::Parser,
	shell_integration::{self, decode_uri_path},
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/tmp".into())
}
fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}
fn row(term: &Term, row: usize) -> String {
	term.cells[row * term.cols..(row + 1) * term.cols]
		.iter()
		.map(|c| c.ch)
		.collect()
}

#[test]
fn dec_private_modes_are_read_from_intermediates() {
	let mut t = terminal(20, 5);
	feed(&mut t, b"\x1b[?1;25;1002;1006;2004h");
	assert!(t.application_cursor && t.cursor_visible && t.mouse_sgr && t.bracketed_paste);
	assert_eq!(t.mouse_tracking, 1002);
	feed(&mut t, b"\x1b[?1;25;1002;1006;2004l");
	assert!(!t.application_cursor && !t.cursor_visible && !t.mouse_sgr && !t.bracketed_paste);
	assert_eq!(t.mouse_tracking, 0);
}

#[test]
fn alternate_screen_restores_shell_and_does_not_enter_history() {
	let mut t = terminal(8, 4);
	feed(&mut t, b"shell\x1b[2;3r\x1b[?6h\x1b[?1049h");
	assert!(t.primary_screen.is_some());
	assert_eq!((t.scroll_top, t.scroll_bottom), (0, 3));
	feed(&mut t, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
	assert!(t.scrollback.is_empty());
	feed(&mut t, b"\x1b[?1049l");
	assert_eq!(row(&t, 0), "shell   ");
	assert_eq!((t.scroll_top, t.scroll_bottom), (1, 2));
	assert!(t.origin_mode);
}

#[test]
fn right_margin_wrap_is_deferred_and_cancelled_by_cr_or_movement() {
	let mut t = terminal(4, 3);
	feed(&mut t, b"abcd");
	assert_eq!((t.cx, t.cy), (3, 0));
	assert!(t.wrap_pending);
	feed(&mut t, b"\x1b[31me");
	assert_eq!(row(&t, 1), "e   ");
	feed(&mut t, b"fgh\rZ");
	assert_eq!(row(&t, 1), "Zfgh");
	feed(&mut t, b"\x1b[1;4Hq\x1b[D!");
	assert_eq!(row(&t, 0), "ab!q");
}

#[test]
fn no_wrap_overwrites_last_column() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"\x1b[?7labcdef");
	assert_eq!(row(&t, 0), "abcf");
	assert_eq!(t.cy, 0);
}

#[test]
fn dec_line_drawing_supports_g0_and_g1() {
	let mut t = terminal(12, 2);
	feed(&mut t, b"\x1b(0lqqk\x1b(Btext\x1b)0\x0ex\x0fq");
	assert_eq!(row(&t, 0), "┌──┐text│q  ");
}

#[test]
fn erased_cells_keep_the_current_background() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"abcd\x1b[44m\r\x1b[K");
	assert!(
		t.cells[..4]
			.iter()
			.all(|cell| cell.ch == ' ' && cell.bg == t.palette[4])
	);
}

#[test]
fn erase_scrollback_does_not_erase_visible_screen() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"one\r\ntwo\r\nlast");
	assert!(!t.scrollback.is_empty());
	let visible = row(&t, 1);
	feed(&mut t, b"\x1b[3J");
	assert!(t.scrollback.is_empty());
	assert_eq!(row(&t, 1), visible);
}

#[test]
fn restricted_scroll_regions_do_not_modify_rows_outside_them() {
	let mut t = terminal(4, 4);
	feed(&mut t, b"top\x1b[4;1Hlast\x1b[2;3r\x1b[4;1H\n\x1b[L\x1b[M");
	assert_eq!(row(&t, 0), "top ");
	assert_eq!(row(&t, 3), "last");
	assert!(t.scrollback.is_empty());
}

#[test]
fn scrollback_anchor_and_limit_are_preserved() {
	let mut t = terminal(4, 2);
	t.scrollback_limit = 3;
	feed(&mut t, b"a\r\nb\r\nc");
	t.scroll_view(1);
	feed(&mut t, b"\r\nd");
	assert_eq!(t.scroll_offset, 2);
	feed(&mut t, b"\r\ne\r\nf");
	assert_eq!(t.scrollback.len(), 3);
	assert_eq!(t.scroll_offset, 3);
	t.scrollback_limit = 0;
	feed(&mut t, b"\r\ng");
	assert!(t.scrollback.is_empty());
	assert_eq!(t.scroll_offset, 0);
}

#[test]
fn cursor_queries_remain_inside_screen_after_full_width_output() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"abcd\x1b[6n");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[1;4R");
	feed(&mut t, b"\x1b[K\x1b[@\x1b[P\x1b[X");
	assert_eq!(t.cells.len(), 8);
}

#[test]
fn partial_parser_chunks_and_wide_overwrite() {
	let mut t = terminal(8, 2);
	let mut parser = Parser::new();
	for byte in "\x1b[?2004h猫abc".bytes() {
		parser.advance(&mut t, &[byte]);
	}
	assert!(t.bracketed_paste);
	assert_eq!(t.cells[0].width, 2);
	feed(&mut t, b"\x1b[2Gx");
	assert_eq!(t.cells[0].ch, ' ');
	assert_eq!(t.cells[1].ch, 'x');
	assert_eq!(t.cells[1].width, 1);
}

#[test]
fn combining_marks_and_joined_emoji_stay_with_their_base_cell() {
	let mut t = terminal(16, 2);
	feed(&mut t, "e\u{301}👨‍👩‍👧‍👦".as_bytes());
	assert_eq!(t.cells[0].ch, 'e');
	assert_eq!(t.cells[0].combining[0], '\u{301}');
	assert_eq!(t.cells[1].ch, '👨');
	assert_eq!(t.cells[1].width, 2);
	assert!(t.cells[1].combining[..t.cells[1].combining_len as usize].contains(&'👩'));
}

#[test]
fn osc8_hyperlinks_are_attached_to_printed_cells_and_can_be_closed() {
	let mut t = terminal(24, 2);
	feed(
		&mut t,
		b"\x1b]8;id=docs;https://example.com/docs\x1b\\docs\x1b]8;;\x1b\\ plain",
	);
	let link = t.cells[0].hyperlink;
	assert_ne!(link, 0);
	assert_eq!(t.hyperlinks[link as usize], "https://example.com/docs");
	assert_eq!(t.cells[5].hyperlink, 0);
}

#[test]
fn kitty_keyboard_mode_can_be_enabled_and_restored() {
	let mut t = terminal(12, 2);
	feed(&mut t, b"\x1b[>1u");
	assert!(t.kitty_keyboard);
	feed(&mut t, b"\x1b[<u");
	assert!(!t.kitty_keyboard);
}

#[test]
fn unsupported_private_sequences_do_not_execute_normal_commands() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"abcd\x1b[?2J");
	assert_eq!(row(&t, 0), "abcd");
}

#[test]
fn uri_decoding_handles_unicode_and_rejects_malformed_paths() {
	assert_eq!(
		decode_uri_path("/tmp/%E7%8C%AB%23x%25"),
		Some("/tmp/猫#x%".into())
	);
	for path in [
		"relative", "/bad%", "/bad%gg", "/bad%00", "/bad%0a", "/bad%ff",
	] {
		assert!(decode_uri_path(path).is_none(), "{path}");
	}
}

#[test]
fn randomized_control_streams_keep_grid_and_cursor_in_bounds() {
	let mut t = terminal(12, 6);
	let mut parser = Parser::new();
	let mut state = 123456789u32;
	let sequences: &[&[u8]] = &[
		b"text",
		"猫".as_bytes(),
		b"\r\n",
		b"\x08",
		b"\t",
		b"\x1b[999A",
		b"\x1b[999B",
		b"\x1b[999C",
		b"\x1b[999D",
		b"\x1b[999P",
		b"\x1b[999@",
		b"\x1b[999X",
		b"\x1b[999L",
		b"\x1b[999M",
		b"\x1b[2;4r",
		b"\x1b[r",
		b"\x1b[?6h",
		b"\x1b[?6l",
		b"\x1b[?1049h",
		b"\x1b[?1049l",
		b"\x1b[2J",
		b"\x1b[1K",
	];
	for index in 0..10_000 {
		state = state.wrapping_mul(1664525).wrapping_add(1013904223);
		parser.advance(&mut t, sequences[state as usize % sequences.len()]);
		if index % 37 == 0 {
			t.resize((state as usize % 20) + 1, (state as usize % 8) + 1);
		}
		assert_eq!(t.cells.len(), t.cols * t.rows);
		assert!(t.cx < t.cols && t.cy < t.rows);
		assert!(t.scroll_top <= t.scroll_bottom && t.scroll_bottom < t.rows);
	}
}

#[test]
fn programmable_tabs_and_back_tabs() {
	let mut t = terminal(24, 2);
	feed(&mut t, b"\t");
	assert_eq!(t.cx, 8);
	feed(&mut t, b"\x1b[3g\r\x1b[5G\x1bH\r\t");
	assert_eq!(t.cx, 4);
	feed(&mut t, b"\x1b[I");
	assert_eq!(t.cx, 23);
	feed(&mut t, b"\x1b[Z");
	assert_eq!(t.cx, 4);
}

#[test]
fn erase_and_resize_do_not_leave_half_wide_characters() {
	let mut t = terminal(4, 2);
	feed(&mut t, "猫猫".as_bytes());
	feed(&mut t, b"\x1b[2G\x1b[X");
	assert_eq!(t.cells[0].ch, ' ');
	assert_eq!(t.cells[1].width, 1);
	t.resize(3, 2);
	assert_eq!(t.cells[2].ch, ' ');
	assert_eq!(t.cells[2].width, 1);
}

#[test]
fn truecolor_colon_and_semicolon_forms_match() {
	let mut t = terminal(10, 2);
	feed(&mut t, b"\x1b[38;2;12;34;56mA\x1b[38:2::12:34:56mB\x1b[38:2:12:34:56mC\x1b[48:2::9:8:7mD\x1b[38:5:196mE");
	assert!(t.cells[..3].iter().all(|c| c.fg == 0x0c2238));
	assert_eq!(t.cells[3].bg, 0x090807);
	assert_eq!(t.cells[4].fg, 0xff0000);
}

#[test]
fn stateful_vte_parser_handles_fragmented_controls_and_invalid_utf8() {
	let mut t = terminal(16, 2);
	let mut parser = Parser::new();
	for byte in b"before\x1b]2;fragmented title\x1b\\\xf0\x9f\x90\x88\xffX" {
		parser.advance(&mut t, &[*byte]);
	}
	assert_eq!(t.title, "fragmented title");
	assert_eq!(t.cells[6].ch, '🐈');
	assert_eq!(t.cells[8].ch, '�');
	assert_eq!(t.cells[9].ch, 'X');
}

#[test]
fn osc_palette_and_default_color_queries_are_answered() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b]4;1;#123456\x07\x1b[31mA");
	assert_eq!(t.cells[0].fg, 0x123456);
	feed(&mut t, b"\x1b]4;1;?\x1b\\\x1b]10;?\x07");
	assert_eq!(t.responses[0], b"\x1b]4;1;rgb:12/34/56\x1b\\");
	assert_eq!(
		t.responses[1],
		format!(
			"\x1b]10;rgb:{:02x}/{:02x}/{:02x}\x1b\\",
			t.default_fg >> 16,
			(t.default_fg >> 8) & 0xff,
			t.default_fg & 0xff
		)
		.as_bytes()
	);
	feed(&mut t, b"\x1b]11;#010203\x07");
	assert_eq!(t.default_bg, 0x010203);
}

#[test]
fn osc_palette_supports_extended_slots_and_reset() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b]4;196;rgb:12/34/56\x07\x1b[38;5;196mX");
	assert_eq!(t.cells[0].fg, 0x123456);
	feed(&mut t, b"\x1b]104;196\x07\x1b[38;5;196mY");
	assert_ne!(t.cells[1].fg, 0x123456);
}

#[test]
fn sgr_extended_styles_clear_independently() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b[4:2;5;53mA\x1b[24;25;55mB");
	assert_eq!(t.cells[0].attrs & (4 | 32 | 64 | 128), 4 | 32 | 64 | 128);
	assert_eq!(t.cells[1].attrs & (4 | 32 | 64 | 128), 0);
}

#[test]
fn ansi_insert_mode_shifts_cells_at_the_cursor() {
	let mut t = terminal(6, 2);
	feed(&mut t, b"abcd\r\x1b[4hZ");
	assert_eq!(row(&t, 0), "Zabcd ");
	feed(&mut t, b"\x1b[4l\rY");
	assert_eq!(row(&t, 0), "Yabcd ");
}

#[test]
fn text_styles_are_set_and_reset_independently() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b[2;3;4;9mA\x1b[22;23;24;29mB\x1b[8mC\x1b[0mD");
	assert_eq!(t.cells[0].attrs, 1 | 2 | 4 | 16);
	assert_eq!(t.cells[1].attrs, 0);
	assert_eq!(t.cells[2].fg, t.cells[2].bg);
	assert_eq!(t.cells[3].attrs, 0);
}

#[test]
fn synchronized_output_is_committed_atomically_with_protocol_replies() {
	let mut t = terminal(8, 3);
	feed(
		&mut t,
		b"before\r\x1b[?2026h\x1b[2Jafter\x1b[?2026$p\x1b[6n",
	);
	assert_eq!(row(&t, 0), "before  ");
	assert!(t.synchronized.is_some());
	assert_eq!(
		t.responses,
		vec![b"\x1b[?2026;1$y".to_vec(), b"\x1b[1;6R".to_vec()]
	);
	feed(&mut t, b"\x1b[?2026l");
	assert_eq!(row(&t, 0), "after   ");
	assert!(t.synchronized.is_none());
	feed(&mut t, b"\x1b[?2026$p");
	assert_eq!(t.responses.last().unwrap(), b"\x1b[?2026;2$y");
}

#[test]
fn synchronization_timeout_recovers_and_duplicate_begin_is_not_nested() {
	let mut t = terminal(8, 3);
	feed(&mut t, b"\x1b[?2026hA\x1b[?2026hB");
	let deadline = t.sync_deadline.unwrap();
	assert!(!t.finish_sync_if_expired(deadline - std::time::Duration::from_millis(1)));
	assert!(t.finish_sync_if_expired(deadline));
	assert_eq!(row(&t, 0), "AB      ");
	assert!(t.synchronized.is_none());
}

#[test]
fn semantic_markers_record_boundaries_and_exit_status() {
	let mut t = terminal(40, 3);
	feed(
		&mut t,
		b"\x1b]133;A\x07prompt> \x1b]133;B\x1b\\false\x1b]133;C\x07\r\noutput\x1b]133;D;1\x07",
	);
	assert_eq!(t.shell.phase, shell_integration::Phase::Finished);
	assert_eq!(t.shell.last_exit_code, Some(1));
	assert_eq!(t.shell.markers.len(), 4);
	assert_eq!(t.shell.markers[1].col, 8);
	assert_eq!(t.shell.markers[2].col, 13);
	feed(&mut t, b"\x1b]133;Z\x07");
	assert_eq!(t.shell.markers.len(), 4);
	feed(&mut t, b"\x1b]133;D;invalid\x07");
	assert_eq!(t.shell.last_exit_code, None);
	for _ in 0..300 {
		feed(&mut t, b"\x1b]133;A\x07");
	}
	assert_eq!(t.shell.markers.len(), 256);
}

#[test]
fn sixel_dcs_is_incremental_and_cursor_advances_at_termination() {
	let mut t = terminal(20, 10);
	t.cell_height = 6;
	let mut parser = Parser::new();
	for byte in b"\x1bP7;1q#1;2;100;0;0!12~" {
		parser.advance(&mut t, &[*byte]);
	}
	assert!(t.images.is_empty());
	assert_eq!(t.cy, 0);
	parser.advance(&mut t, b"\x1b\\");
	assert_eq!(t.images.len(), 1);
	assert_eq!(t.images[0].bitmap.width, 12);
	assert_eq!(t.images[0].bitmap.pixels[0], 0xff0000);
	assert_eq!((t.cx, t.cy), (0, 1));
	feed(&mut t, b"\x1b[2J");
	assert!(t.images.is_empty());
}

#[test]
fn sixel_obeys_sync_and_alternate_screen_boundaries() {
	let mut t = terminal(20, 10);
	feed(&mut t, b"\x1b[?2026h\x1bP7;1q~\x1b\\");
	assert!(t.images.is_empty());
	feed(&mut t, b"\x1b[?2026l");
	assert_eq!(t.images.len(), 1);
	feed(&mut t, b"\x1b[?1049h");
	assert!(t.images.is_empty());
	feed(&mut t, b"\x1b[?1049l");
	assert_eq!(t.images.len(), 1);
}

#[test]
fn saved_cursor_restores_styles_and_coordinates() {
	let mut t = terminal(8, 3);
	feed(&mut t, b"\x1b[31;4mAB\x1b7\x1b[0m\x1b[3;8H\x1b8C");
	assert_eq!(t.cells[2].ch, 'C');
	assert_eq!(t.cells[2].fg, t.palette[1]);
	assert_eq!(t.cells[2].attrs, 4);
	assert_eq!((t.cx, t.cy), (3, 0));
}

#[test]
fn complete_sixel_color_chart_survives_the_following_prompt() {
	let mut t = terminal(100, 30);
	t.cell_height = 33;
	feed(&mut t, b"\x1bP7;1q\"1;1;180;36#1;2;100;20;20!60~#2;2;20;100;20!60~#3;2;20;20;100!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~\x1b\\\x1b]133;D;7\x07prompt> ");
	assert_eq!(t.images.len(), 1);
	assert_eq!(t.images[0].bitmap.width, 180);
	assert_eq!(t.images[0].bitmap.height, 36);
	assert_eq!(t.cy, 2);
}

#[test]
fn osc_directory_and_bell_are_tracked() {
	let mut term = Term::new(20, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
	Parser::new().advance(&mut term, b"\x1b]7;file://localhost/tmp/my%20dir\x07\x07");
	assert_eq!(term.cwd, std::path::PathBuf::from("/tmp/my dir"));
	assert!(term.bell);
}

#[test]
fn lowering_the_scrollback_limit_discards_oldest_lines_and_clamps_the_view() {
	let mut t = terminal(4, 1);
	feed(&mut t, b"a\r\nb\r\nc\r\nd");
	t.scroll_view(3);
	t.set_scrollback_limit(1);
	assert_eq!(t.scrollback.len(), 1);
	assert_eq!(t.scrollback[0][0].ch, 'c');
	assert_eq!(t.history_base, 2);
	assert_eq!(t.scroll_offset, 1);
}
