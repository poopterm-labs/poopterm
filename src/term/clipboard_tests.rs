//! Clipboard writes: OSC 52 selection targets and kitty's OSC 5522, from
//! bytes in to queued writes and replies out.
use super::clipboard::{MAX_WRITE, Selection};
use crate::{
	graphics::base64::encode,
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn replies(term: &mut Term) -> Vec<String> {
	term
		.responses
		.drain(..)
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

/// Queued writes as (selection, [(mime, text)]).
fn writes(term: &mut Term) -> Vec<(Selection, Vec<(String, String)>)> {
	term
		.clipboard_writes
		.drain(..)
		.map(|write| {
			let items = write
				.items
				.iter()
				.map(|(mime, data)| (mime.clone(), String::from_utf8_lossy(data).into_owned()))
				.collect();
			(write.selection, items)
		})
		.collect()
}

fn text(value: &str) -> Vec<(String, String)> {
	vec![("text/plain".to_owned(), value.to_owned())]
}

fn b64(text: &str) -> String {
	encode(text.as_bytes(), true)
}

#[test]
fn osc52_writes_each_named_selection_and_clears_on_invalid_data() {
	let mut t = terminal();
	send(&mut t, &format!("\x1b]52;c;{}\x07", b64("hi")));
	assert_eq!(writes(&mut t), [(Selection::Clipboard, text("hi"))]);
	// An empty target list means s0; s is the clipboard, cut buffers and the
	// secondary selection do not exist here.
	send(
		&mut t,
		&format!("\x1b]52;;{}\x07\x1b]52;s0;{0}\x07", b64("a")),
	);
	assert_eq!(
		writes(&mut t),
		[
			(Selection::Clipboard, text("a")),
			(Selection::Clipboard, text("a"))
		]
	);
	send(
		&mut t,
		&format!("\x1b]52;pc;{}\x07\x1b]52;q7;{0}\x07", b64("b")),
	);
	assert_eq!(
		writes(&mut t),
		[
			(Selection::Primary, text("b")),
			(Selection::Clipboard, text("b"))
		]
	);
	send(&mut t, "\x1b]52;c;not base64!\x07");
	assert_eq!(writes(&mut t), [(Selection::Clipboard, text(""))]);
	// Reads are refused without a reply.
	send(&mut t, "\x1b]52;c;?\x07");
	assert_eq!((writes(&mut t), replies(&mut t)), (vec![], vec![]));
}

#[test]
fn osc5522_assembles_chunked_types_and_aliases() {
	let mut t = terminal();
	let html = b64("<b>hello</b>");
	let (first, rest) = html.split_at(5);
	send(
		&mut t,
		&format!(
			"\x1b]5522;type=write:id=a$b-1\x1b\\\
			 \x1b]5522;type=wdata:mime={};{}\x1b\\\
			 \x1b]5522;type=wdata:mime={};{first}\x1b\\\
			 \x1b]5522;type=wdata:mime={2};{rest}\x1b\\\
			 \x1b]5522;type=walias:mime={2};{}\x1b\\\
			 \x1b]5522;type=wdata\x1b\\",
			b64("text/plain"),
			b64("hello"),
			b64("text/html"),
			b64("application/xhtml+xml image/x-html"),
		),
	);
	assert_eq!(
		replies(&mut t),
		["\x1b]5522;type=write:status=DONE:id=ab-1\x1b\\"]
	);
	let html = |mime: &str| (mime.to_owned(), "<b>hello</b>".to_owned());
	assert_eq!(
		writes(&mut t),
		[(
			Selection::Clipboard,
			vec![
				("text/plain".to_owned(), "hello".to_owned()),
				html("text/html"),
				html("application/xhtml+xml"),
				html("image/x-html"),
			]
		)]
	);
}

#[test]
fn osc5522_errors_abort_the_write_until_the_next_one() {
	let mut t = terminal();
	let mime = b64("text/plain");
	send(
		&mut t,
		&format!(
			"\x1b]5522;type=write\x1b\\\x1b]5522;type=wdata:mime={mime};a b=\x1b\\\
			 \x1b]5522;type=wdata:mime={mime};{}\x1b\\\x1b]5522;type=wdata\x1b\\",
			b64("x")
		),
	);
	assert_eq!(
		replies(&mut t),
		["\x1b]5522;type=write:status=EINVAL\x1b\\"]
	);
	assert!(writes(&mut t).is_empty());
	// Data must be padded once complete; a missing MIME type is invalid too.
	for packet in ["mime=dGV4dA;eA", &format!(";{}", b64("x"))] {
		send(
			&mut t,
			&format!(
				"\x1b]5522;type=write\x1b\\\x1b]5522;type=wdata:{packet}\x1b\\\x1b]5522;type=wdata\x1b\\"
			),
		);
		assert_eq!(
			replies(&mut t),
			["\x1b]5522;type=write:status=EINVAL\x1b\\"],
			"{packet}"
		);
	}
	// Data without a write is ignored.
	send(
		&mut t,
		&format!("\x1b]5522;type=wdata:mime={mime};{}\x1b\\", b64("x")),
	);
	assert!(replies(&mut t).is_empty());
}

#[test]
fn osc5522_limits_writes_and_refuses_reads() {
	let mut t = terminal();
	let chunk = b64(&"x".repeat(3000));
	let packets = MAX_WRITE / 3000 + 1;
	let mut input = "\x1b]5522;type=write\x1b\\".to_owned();
	for _ in 0..packets {
		input += &format!(
			"\x1b]5522;type=wdata:mime={};{chunk}\x1b\\",
			b64("text/plain")
		);
	}
	send(&mut t, &input);
	assert_eq!(replies(&mut t), ["\x1b]5522;type=write:status=EFBIG\x1b\\"]);
	send(
		&mut t,
		&format!("\x1b]5522;type=read:id=7;{}\x1b\\", b64("text/plain")),
	);
	assert_eq!(
		replies(&mut t),
		["\x1b]5522;type=read:status=EPERM:id=7\x1b\\"]
	);
}

#[test]
fn osc5522_limits_the_number_of_types() {
	let mut t = terminal();
	let mut input = "\x1b]5522;type=write\x1b\\".to_owned();
	for index in 0..65 {
		input += &format!(
			"\x1b]5522;type=wdata:mime={};{}\x1b\\",
			b64(&format!("a/{index}")),
			b64("x")
		);
	}
	send(&mut t, &input);
	assert_eq!(replies(&mut t), ["\x1b]5522;type=write:status=EFBIG\x1b\\"]);
}

#[test]
fn osc_replies_are_not_held_back_by_synchronized_output() {
	let mut t = terminal();
	send(&mut t, "\x1b[?2026h\x1b]10;?\x07");
	assert_eq!(replies(&mut t).len(), 1);
}
