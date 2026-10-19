//! Kitty graphics protocol emulation tests: APC commands in, placements,
//! stored images, cursor and replies out.
use crate::{
	bench::images::base64,
	graphics::{Extent, KittyId, Rect},
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

/// A terminal with 10×20 pixel cells.
fn terminal(cols: usize, rows: usize) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
	term.cell_width = 10;
	term.cell_height = 20;
	term
}

fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}

/// One graphics command with `payload` base64-encoded.
fn command(control: &str, payload: &[u8]) -> Vec<u8> {
	let mut bytes = format!("\x1b_G{control};").into_bytes();
	bytes.extend(base64(payload));
	bytes.extend_from_slice(b"\x1b\\");
	bytes
}

/// Opaque RGB pixels for a `width`×`height` image.
fn rgb(width: usize, height: usize) -> Vec<u8> {
	(0..width * height).flat_map(|i| [i as u8, 2, 3]).collect()
}

/// Transmits a `width`×`height` RGB image with extra control keys.
fn send(term: &mut Term, control: &str, width: usize, height: usize) {
	let control = format!("f=24,s={width},v={height},{control}");
	feed(term, &command(&control, &rgb(width, height)));
}

fn replies(term: &mut Term) -> Vec<String> {
	term
		.responses
		.drain(..)
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

/// `(image id, placement id, col, row)` of every placement.
fn placements(term: &Term) -> Vec<(u32, u32, usize, isize)> {
	term
		.images
		.iter()
		.map(|placement| {
			let id = placement.kitty.unwrap();
			(id.image, id.placement, placement.col, placement.row)
		})
		.collect()
}

#[test]
fn transmit_and_display_places_at_the_cursor_and_moves_it() {
	let mut t = terminal(20, 10);
	feed(&mut t, b"\x1b[3;5H");
	send(&mut t, "a=T,i=1", 15, 30);
	assert_eq!(placements(&t), [(1, 0, 4, 2)]);
	let placement = &t.images[0];
	assert_eq!(placement.bitmap.width, 15);
	assert_eq!(placement.extent, Extent::Natural);
	// 15×30 pixels cover 2×2 cells: right of the image, on its last row.
	assert_eq!((t.cx, t.cy), (6, 3));
	assert_eq!(replies(&mut t), ["\x1b_Gi=1;OK\x1b\\"]);
	// C=1 keeps the cursor; q=1 keeps quiet about success.
	send(&mut t, "a=T,i=2,C=1,q=1", 15, 30);
	assert_eq!((t.cx, t.cy), (6, 3));
	assert!(replies(&mut t).is_empty());
}

#[test]
fn displaying_at_the_bottom_scrolls_the_image_up_with_the_text() {
	let mut t = terminal(20, 4);
	feed(&mut t, b"\x1b[4;1H");
	send(&mut t, "a=T,i=1,q=2", 10, 50);
	// Three rows: the screen scrolls twice so the image ends on the last row.
	assert_eq!(placements(&t), [(1, 0, 0, 1)]);
	assert_eq!((t.cx, t.cy), (1, 3));
}

#[test]
fn transmit_stores_without_placing_and_place_shows_later() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=t,i=4", 8, 8);
	assert!(t.images.is_empty());
	assert_eq!(replies(&mut t), ["\x1b_Gi=4;OK\x1b\\"]);
	feed(
		&mut t,
		b"\x1b[2;3H\x1b_Ga=p,i=4,p=9,x=2,y=1,w=4,h=6,c=3,r=2,X=5,Y=30,z=-1,C=1\x1b\\",
	);
	assert_eq!(replies(&mut t), ["\x1b_Gi=4,p=9;OK\x1b\\"]);
	assert_eq!(placements(&t), [(4, 9, 2, 1)]);
	let placement = &t.images[0];
	assert_eq!(
		placement.source,
		Rect {
			x: 2,
			y: 1,
			width: 4,
			height: 6
		}
	);
	assert_eq!(placement.extent, Extent::Cells { cols: 3, rows: 2 });
	// Offsets stay inside the first cell.
	assert_eq!(placement.offset, (5, 19));
	assert_eq!(placement.z, -1);
	// The same placement id replaces; another one adds a placement.
	feed(
		&mut t,
		b"\x1b[5;1H\x1b_Ga=p,i=4,p=9,q=1\x1b\\\x1b_Ga=p,i=4,p=2,q=1,C=1\x1b\\",
	);
	assert_eq!(placements(&t), [(4, 9, 0, 4), (4, 2, 1, 4)]);
	// Without a placement id, every put adds one.
	feed(&mut t, b"\x1b_Ga=p,i=4,q=1\x1b\\\x1b_Ga=p,i=4,q=1\x1b\\");
	assert_eq!(t.images.len(), 4);
}

#[test]
fn source_rectangles_are_clamped_and_one_side_keeps_the_aspect_ratio() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=t,i=1,q=2", 40, 20);
	feed(&mut t, b"\x1b_Ga=p,i=1,x=30,w=50,h=10,c=2,q=2\x1b\\");
	let placement = &t.images[0];
	assert_eq!(placement.source.width, 10);
	assert_eq!(placement.extent, Extent::Cols(2));
	// 2 columns of 10 pixels: 20×20 pixels for a square source.
	assert_eq!(placement.size((10, 20)), (20, 20));
	feed(&mut t, b"\x1b_Ga=p,i=1,r=2,q=2\x1b\\");
	assert_eq!(t.images[1].size((10, 20)), (80, 40));
}

#[test]
fn errors_are_reported_with_the_image_id() {
	let mut t = terminal(20, 10);
	feed(&mut t, b"\x1b_Ga=p,i=7\x1b\\");
	assert_eq!(replies(&mut t), ["\x1b_Gi=7;ENOENT:image not found\x1b\\"]);
	feed(&mut t, &command("a=T,i=3,f=24,s=4,v=4", &[1, 2, 3]));
	assert_eq!(
		replies(&mut t),
		["\x1b_Gi=3;ENODATA:insufficient image data\x1b\\"]
	);
	assert!(t.images.is_empty());
	feed(&mut t, b"\x1b_Ga=p,i=1,I=2\x1b\\");
	assert_eq!(
		replies(&mut t)[0],
		"\x1b_Gi=1,I=2;EINVAL:both i and I given\x1b\\"
	);
	feed(&mut t, b"\x1b_Ga=p,i=7,q=2\x1b\\\x1b_Ga=y,i=7\x1b\\");
	assert_eq!(replies(&mut t), ["\x1b_Gi=7;EINVAL:unknown action\x1b\\"]);
	// Commands without an id get no reply at all.
	feed(&mut t, b"\x1b_Ga=p\x1b\\");
	assert!(replies(&mut t).is_empty());
}

#[test]
fn image_numbers_get_a_fresh_id_and_find_the_newest_image() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=t,I=5", 2, 2);
	let first = replies(&mut t).remove(0);
	let id: u32 = first["\x1b_Gi=".len()..first.find(',').unwrap()]
		.parse()
		.unwrap();
	assert_eq!(first, format!("\x1b_Gi={id},I=5;OK\x1b\\"));
	send(&mut t, "a=t,I=5,q=2", 3, 3);
	feed(&mut t, b"\x1b_Ga=p,I=5\x1b\\");
	let newest = t.images[0].kitty.unwrap().image;
	assert_ne!(newest, id);
	assert_eq!(t.images[0].bitmap.width, 3);
	assert_eq!(replies(&mut t), [format!("\x1b_Gi={newest},I=5;OK\x1b\\")]);
	// Deleting by number acts on the newest image only.
	feed(&mut t, format!("\x1b_Ga=p,i={id},q=2\x1b\\").as_bytes());
	feed(&mut t, b"\x1b_Ga=d,d=n,I=5\x1b\\");
	assert_eq!(placements(&t).len(), 1);
	assert_eq!(t.images[0].kitty.unwrap().image, id);
	feed(&mut t, b"\x1b_Ga=d,d=N,I=5\x1b\\");
	assert!(t.kitty_images.get(newest).is_none());
	assert!(t.kitty_images.get(id).is_some());
}

#[test]
fn query_checks_an_image_without_storing_it() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=q,i=31", 1, 1);
	assert_eq!(replies(&mut t), ["\x1b_Gi=31;OK\x1b\\"]);
	feed(&mut t, b"\x1b_Ga=p,i=31\x1b\\");
	assert_eq!(replies(&mut t), ["\x1b_Gi=31;ENOENT:image not found\x1b\\"]);
	feed(&mut t, &command("a=q,i=31,f=100", b"not png"));
	assert!(replies(&mut t)[0].starts_with("\x1b_Gi=31;EBADPNG:"));
}

#[test]
fn chunked_transmissions_survive_split_reads() {
	let mut t = terminal(20, 10);
	let encoded = base64(&rgb(4, 4));
	let (first, rest) = encoded.split_at(20);
	let mut stream = b"\x1b_Ga=T,f=24,s=4,v=4,i=2,m=1;".to_vec();
	stream.extend_from_slice(first);
	stream.extend_from_slice(b"\x1b\\\x1b_Gm=0;");
	stream.extend_from_slice(rest);
	stream.extend_from_slice(b"\x1b\\");
	let mut parser = Parser::new();
	for piece in stream.chunks(7) {
		parser.advance(&mut t, piece);
	}
	assert_eq!(placements(&t), [(2, 0, 0, 0)]);
	assert_eq!(t.images[0].bitmap.pixels[1], 0xff010203);
}

#[test]
fn text_and_erasing_leave_images_but_clear_screen_removes_them() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,i=1,q=2,C=1", 30, 40);
	feed(&mut t, b"hello\x1b[K\x1b[1J\x1b[2K\x1b[P\x1b[M");
	assert_eq!(t.images.len(), 1);
	feed(&mut t, b"\x1b[2J");
	assert!(t.images.is_empty());
	// The image data stays and can be placed again.
	feed(&mut t, b"\x1b_Ga=p,i=1,q=2\x1b\\");
	assert_eq!(t.images.len(), 1);
	feed(&mut t, b"\x1bc");
	assert!(t.images.is_empty());
}

#[test]
fn images_scroll_into_history_and_are_dropped_with_it() {
	let mut t = terminal(20, 4);
	t.scrollback_limit = 5;
	send(&mut t, "a=T,i=1,q=2,C=1", 10, 20);
	feed(&mut t, b"\x1b[4;1H\n\n");
	assert_eq!(placements(&t), [(1, 0, 0, -2)]);
	feed(&mut t, b"\n\n\n");
	assert_eq!(placements(&t), [(1, 0, 0, -5)]);
	feed(&mut t, b"\n");
	assert!(t.images.is_empty());
}

#[test]
fn alternate_screen_has_its_own_placements() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,i=1,q=2", 10, 20);
	feed(&mut t, b"\x1b[?1049h");
	assert!(t.images.is_empty());
	feed(&mut t, b"\x1b_Ga=p,i=1,q=2\x1b\\");
	assert_eq!(t.images.len(), 1);
	feed(&mut t, b"\x1b[?1049l");
	assert_eq!(t.images.len(), 1);
}

#[test]
fn commands_inside_synchronized_output_apply_when_it_ends() {
	let mut t = terminal(20, 10);
	let encoded = base64(&rgb(2, 2));
	let mut stream = b"\x1b_Ga=T,f=24,s=2,v=2,i=1,m=1;".to_vec();
	stream.extend_from_slice(&encoded[..8]);
	stream.extend_from_slice(b"\x1b\\\x1b[?2026h\x1b_Gm=0;");
	stream.extend_from_slice(&encoded[8..]);
	stream.extend_from_slice(b"\x1b\\");
	feed(&mut t, &stream);
	assert!(t.images.is_empty());
	// Replies are not held back.
	assert_eq!(replies(&mut t), ["\x1b_Gi=1;OK\x1b\\"]);
	feed(&mut t, b"\x1b[?2026l");
	assert_eq!(placements(&t), [(1, 0, 0, 0)]);
}

/// Places image 1 three times and image 2 once on a 20×10 screen:
/// (1,1) at 0,0 z=0; (1,2) at 5,2 z=3; (1,0) at 10,5 z=3; (2,0) at 0,8 z=0.
fn scene() -> Term {
	let mut t = terminal(20, 10);
	send(&mut t, "a=t,i=1,q=2", 20, 40);
	send(&mut t, "a=t,i=2,q=2", 10, 20);
	feed(
		&mut t,
		b"\x1b_Ga=p,i=1,p=1,q=2,C=1\x1b\\\x1b[3;6H\x1b_Ga=p,i=1,p=2,z=3,q=2,C=1\x1b\\\
		\x1b[6;11H\x1b_Ga=p,i=1,z=3,q=2,C=1\x1b\\\x1b[9;1H\x1b_Ga=p,i=2,q=2,C=1\x1b\\",
	);
	assert_eq!(t.images.len(), 4);
	t
}

#[test]
fn delete_selects_placements_in_every_way_the_protocol_defines() {
	// Delete keys, and the (image, placement) ids left afterwards.
	type Case = (&'static [u8], &'static [(u32, u32)]);
	let cases: [Case; 12] = [
		(b"", &[]),
		(b"d=a", &[]),
		(b"d=i,i=1", &[(2, 0)]),
		(b"d=i,i=1,p=2", &[(1, 1), (1, 0), (2, 0)]),
		(b"d=p,x=7,y=4", &[(1, 1), (1, 0), (2, 0)]),
		(b"d=q,x=7,y=4,z=0", &[(1, 1), (1, 2), (1, 0), (2, 0)]),
		(b"d=r,x=2,y=9", &[(1, 1), (1, 2), (1, 0)]),
		(b"d=x,x=2", &[(1, 2), (1, 0), (2, 0)]),
		(b"d=y,y=6", &[(1, 1), (1, 2), (2, 0)]),
		(b"d=z,z=3", &[(1, 1), (2, 0)]),
		(b"d=c", &[(1, 2), (1, 0), (2, 0)]),
		(b"d=I,i=9", &[(1, 1), (1, 2), (1, 0), (2, 0)]),
	];
	for (keys, left) in cases {
		let mut t = scene();
		// The cursor is in the top-left cell for d=c.
		feed(&mut t, &[b"\x1b[H\x1b_Ga=d,", keys, b"\x1b\\"].concat());
		let ids: Vec<(u32, u32)> = placements(&t).iter().map(|&(i, p, ..)| (i, p)).collect();
		assert_eq!(ids, left, "{}", String::from_utf8_lossy(keys));
		assert!(replies(&mut t).is_empty());
	}
}

#[test]
fn uppercase_deletes_free_images_without_placements() {
	let mut t = scene();
	feed(&mut t, b"\x1b_Ga=d,d=y,y=9\x1b\\");
	assert!(t.kitty_images.get(2).is_some());
	feed(&mut t, b"\x1b_Ga=d,d=Y,y=6\x1b\\");
	// Image 1 still has placements, so its data stays.
	assert!(t.kitty_images.get(1).is_some());
	assert_eq!(t.images.len(), 2);
	feed(&mut t, b"\x1b_Ga=d,d=A\x1b\\");
	assert!(t.kitty_images.get(1).is_none());
	feed(&mut t, b"\x1b_Ga=d,d=I,i=2\x1b\\");
	assert!(t.kitty_images.get(2).is_none());
}

#[test]
fn relative_placements_follow_their_parent_and_die_with_it() {
	let mut t = scene();
	feed(&mut t, b"\x1b_Ga=p,i=2,p=5,P=1,Q=2,H=3,V=-1\x1b\\");
	assert_eq!(replies(&mut t), ["\x1b_Gi=2,p=5;OK\x1b\\"]);
	assert_eq!(placements(&t)[4], (2, 5, 8, 1));
	assert_eq!(
		t.images[4].parent,
		Some(KittyId {
			image: 1,
			placement: 2
		})
	);
	// Chains of children are fine, the parent must exist, and no cycles.
	feed(&mut t, b"\x1b_Ga=p,i=2,p=6,P=2,Q=5,q=1\x1b\\");
	feed(&mut t, b"\x1b_Ga=p,i=2,p=7,P=1,Q=99\x1b\\");
	assert!(replies(&mut t)[0].starts_with("\x1b_Gi=2,p=7;ENOPARENT:"));
	feed(&mut t, b"\x1b_Ga=p,i=1,p=2,P=2,Q=6\x1b\\");
	assert!(replies(&mut t)[0].starts_with("\x1b_Gi=1,p=2;ECYCLE:"));
	assert_eq!(t.images.len(), 6);
	feed(&mut t, b"\x1b_Ga=d,d=i,i=1,p=2\x1b\\");
	assert_eq!(t.images.len(), 3);
}

#[test]
fn relative_chains_have_a_depth_limit() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=t,i=1,q=2", 1, 1);
	feed(&mut t, b"\x1b_Ga=p,i=1,p=1,q=2\x1b\\");
	for p in 2..=9 {
		feed(
			&mut t,
			format!("\x1b_Ga=p,i=1,p={p},P=1,Q={},q=1\x1b\\", p - 1).as_bytes(),
		);
	}
	assert!(replies(&mut t).is_empty());
	feed(&mut t, b"\x1b_Ga=p,i=1,p=10,P=1,Q=9\x1b\\");
	assert!(replies(&mut t)[0].starts_with("\x1b_Gi=1,p=10;ETOODEEP:"));
}

#[test]
fn eviction_removes_the_placements_of_evicted_images() {
	let mut t = terminal(20, 10);
	t.kitty_images = crate::graphics::kitty::Store::with_quota(100 * 4);
	send(&mut t, "a=T,i=1,q=2", 10, 10);
	send(&mut t, "a=T,i=2,q=2", 1, 1);
	assert_eq!(t.images.len(), 1);
	assert_eq!(t.images[0].kitty.unwrap().image, 2);
}

/// Row/column diacritics for 0, 1 and 2.
const D: [char; 3] = ['\u{305}', '\u{30d}', '\u{30e}'];
const PLACEHOLDER: char = '\u{10EEEE}';

/// `(screen row, screen col, image row, image col, bitmap width)` of the
/// placeholder cells in a frame.
fn placeholder_cells(term: &Term) -> Vec<(usize, usize, usize, usize, usize)> {
	let mut frame = crate::term::Frame::default();
	term.snapshot(&mut frame);
	frame
		.placeholders
		.iter()
		.map(|cell| {
			(
				cell.row,
				cell.col,
				cell.slot.0,
				cell.slot.1,
				cell.bitmap.width,
			)
		})
		.collect()
}

#[test]
fn virtual_placements_are_stored_without_showing_or_moving_anything() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,U=1,i=5,c=2,r=1", 10, 20);
	assert!(t.images.is_empty());
	assert_eq!((t.cx, t.cy), (0, 0));
	assert_eq!(replies(&mut t), ["\x1b_Gi=5;OK\x1b\\"]);
	feed(&mut t, b"\x1b_Ga=p,U=1,i=5,P=5,Q=1\x1b\\");
	assert!(replies(&mut t)[0].starts_with("\x1b_Gi=5;EINVAL:"));
}

#[test]
fn placeholder_text_shows_slices_of_the_virtual_placement() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,U=1,i=5,c=2,r=2,q=2", 10, 20);
	// 256-color id with bold (which brightens the color, not the id), then
	// a second row inferring its column.
	let text = format!(
		"\x1b[1;38;5;5m{PLACEHOLDER}{}{}{PLACEHOLDER}\r\n{PLACEHOLDER}{}{PLACEHOLDER}x",
		D[0], D[0], D[1]
	);
	feed(&mut t, text.as_bytes());
	assert_eq!(
		placeholder_cells(&t),
		[
			(0, 0, 0, 0, 10),
			(0, 1, 0, 1, 10),
			(1, 0, 1, 0, 10),
			(1, 1, 1, 1, 10)
		]
	);
	// Truecolor ids; cells outside the placement show nothing.
	send(&mut t, "a=T,U=1,i=258,c=1,r=1,q=2", 3, 3);
	feed(
		&mut t,
		format!("\x1b[0;38;2;0;1;2m\x1b[3;1H{PLACEHOLDER}{PLACEHOLDER}").as_bytes(),
	);
	assert_eq!(placeholder_cells(&t)[4..], [(2, 0, 0, 0, 3)]);
	// Deleting the image's placements removes the virtual one too.
	feed(&mut t, b"\x1b_Ga=d,d=i,i=5\x1b\\");
	assert_eq!(placeholder_cells(&t), [(2, 0, 0, 0, 3)]);
}

#[test]
fn virtual_placements_without_a_size_cover_the_image_in_cells() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,U=1,i=7,q=2", 25, 30);
	feed(
		&mut t,
		format!("\x1b[38;5;7m{PLACEHOLDER}{}{}", D[1], D[2]).as_bytes(),
	);
	let mut frame = crate::term::Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.placeholders[0].cells, (3, 2));
}

#[test]
fn basic_and_bright_colors_name_images_0_to_15() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,U=1,i=3,q=2", 10, 20);
	send(&mut t, "a=T,U=1,i=13,q=2", 10, 20);
	let text = format!("\x1b[33m{PLACEHOLDER}\x1b[95m{PLACEHOLDER}\x1b[39m{PLACEHOLDER}");
	feed(&mut t, text.as_bytes());
	assert_eq!(placeholder_cells(&t), [(0, 0, 0, 0, 10), (0, 1, 0, 0, 10)]);
	// The id survives saving and restoring the cursor.
	feed(
		&mut t,
		format!("\x1b[33m\x1b7\x1b[m\x1b8\r{PLACEHOLDER}").as_bytes(),
	);
	assert_eq!(placeholder_cells(&t)[0], (0, 0, 0, 0, 10));
}
