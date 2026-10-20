//! Sixel transfers and decoding, checked against the reference decoder.
use super::*;

#[test]
fn extending_matches_pushing_byte_by_byte_up_to_the_limit() {
	let data: Vec<u8> = (0..MAX_PAYLOAD + 10)
		.map(|i| b'?' + (i % 63) as u8)
		.collect();
	for split in [0, 1, MAX_PAYLOAD - 1, MAX_PAYLOAD, MAX_PAYLOAD + 3] {
		let (mut one, mut bulk) = (Transfer::new(7, 1), Transfer::new(7, 1));
		for &byte in &data[..split] {
			one.push(byte);
		}
		bulk.extend(&data[..split]);
		assert_eq!(
			(&one.bytes, one.overflow),
			(&bulk.bytes, bulk.overflow),
			"{split}"
		);
	}
	let mut bulk = Transfer::new(7, 1);
	bulk.extend(&data[..MAX_PAYLOAD - 2]);
	bulk.extend(&data[..5]);
	assert!(bulk.overflow);
	assert_eq!(bulk.bytes.len(), MAX_PAYLOAD);
}

fn decode(data: &[u8]) -> Option<Bitmap> {
	let mut t = Transfer::new(7, 1);
	for &byte in data {
		t.push(byte);
	}
	t.finish(0, 0xffffff)
}
#[test]
fn repeats_planes_and_transparency() {
	let image = decode(b"#1;2;100;0;0!3~$#2;2;0;100;0?@").unwrap();
	assert_eq!((image.width, image.height), (3, 6));
	assert_eq!(image.pixels[0], 0xffff0000);
	assert_eq!(image.pixels[1], 0xff00ff00);
	assert_eq!(image.pixels[4], 0xffff0000);
	assert_eq!(decode(b"?").unwrap().pixels[0], 0);
}
#[test]
fn raster_attributes_and_aspect_ratio() {
	let image = decode(b"\"2;1;4;6#1;2;0;0;100@").unwrap();
	assert_eq!((image.width, image.height), (4, 12));
	assert_eq!(image.pixels[0], 0xff0000ff);
	assert_eq!(image.pixels[4], 0xff0000ff);
}
#[test]
fn hls_primary_checks() {
	assert_eq!(hls(0, 50, 100), 0x0000ff);
	assert_eq!(hls(120, 50, 100), 0xff0000);
	assert_eq!(hls(240, 50, 100), 0x00ff00);
}

/// Decodes `stream` with the flat decoder and the reference, and asserts both
/// give the same bitmap (or both refuse it). Returns the bitmap.
fn same_as_reference(
	stream: &[u8],
	aspect: usize,
	background: u32,
	foreground: u32,
) -> Option<Bitmap> {
	let fast = Decoder::new(aspect, background, foreground).decode(stream);
	let slow = reference::Decoder::new(aspect, background, foreground).decode(stream);
	let parts = |image: &Option<Bitmap>| {
		image.as_ref().map(|image| {
			(
				image.width,
				image.height,
				image.opaque,
				image.pixels.clone(),
			)
		})
	};
	assert_eq!(
		parts(&fast),
		parts(&slow),
		"{}",
		String::from_utf8_lossy(stream)
	);
	fast
}

/// A random stream of mostly valid Sixel commands, with some malformed ones.
fn random_stream(rng: &mut crate::bench::corpus::Rng) -> Vec<u8> {
	let mut out = Vec::new();
	for _ in 0..rng.below(80) {
		let data = b'?' + rng.below(64) as u8;
		match rng.below(40) {
			0..=14 => out.push(data),
			15..=21 => {
				let count = rng.pick(&[0, 1, 2, 5, 17, 40, 300]);
				out.extend(format!("!{count}").bytes().chain([data]));
			}
			22..=24 => out.push(b'$'),
			25..=26 => out.push(b'-'),
			27..=29 => out.extend(format!("#{}", rng.pick(&[0, 1, 2, 15, 255])).bytes()),
			30..=32 => {
				let space = rng.pick(&[1, 2, 2, 7]);
				let [a, b, c] = [rng.below(400), rng.below(120), rng.below(120)];
				out.extend(format!("#{};{space};{a};{b};{c}", rng.below(20)).bytes());
			}
			33..=34 => {
				let [p, q, w, h] = [rng.below(4), rng.below(3), rng.below(90), rng.below(40)];
				let raster = rng
					.pick(&[
						format!("{p};{q}"),
						format!("{p};{q};{w};{h}"),
						format!("{p}"),
					])
					.clone();
				out.extend(format!("\"{raster}").bytes());
			}
			35..=37 => out.push(*rng.pick(b" \n\x7f%0123456789;")),
			// Malformed commands, rare enough that most streams are images.
			38 if rng.below(8) == 0 => {
				out.extend_from_slice(rng.pick(&[&b"!3$"[..], b"#256", b"#1;2;3;4;5;6;7", b"!"]))
			}
			_ => out.extend(format!("#{};2;100;100;100!{}~", rng.below(3), rng.below(9)).bytes()),
		}
	}
	out
}

#[test]
fn random_streams_decode_like_the_reference() {
	let mut rng = crate::bench::corpus::Rng::new(7);
	let mut decoded = 0;
	for _ in 0..3000 {
		let stream = random_stream(&mut rng);
		let aspect = *rng.pick(&[1, 2, 3, 5]);
		let background = *rng.pick(&[TRANSPARENT, 0x123456]);
		decoded += same_as_reference(&stream, aspect, background, 0xabcdef).is_some() as usize;
	}
	// Most streams are images, so the comparison covers painting, not refusals.
	assert!(decoded > 1500, "{decoded}");
}

#[test]
fn rows_painted_before_the_image_widens_keep_their_pixels() {
	let image = same_as_reference(b"#1;2;100;0;0~-!9@-#2;2;0;0;100!40~", 1, TRANSPARENT, 0).unwrap();
	assert_eq!((image.width, image.height), (40, 18));
	assert_eq!(image.pixels[..2], [0xffff0000, 0]);
	assert_eq!(image.pixels[5 * 40], 0xffff0000);
	// The second band's row 0 is 9 pixels wide, still red, then transparent.
	assert_eq!(image.pixels[6 * 40 + 8..6 * 40 + 10], [0xffff0000, 0]);
	assert!(
		image.pixels[12 * 40..]
			.iter()
			.all(|&pixel| pixel == 0xff0000ff)
	);
}

#[test]
fn raster_attributes_smaller_or_larger_than_the_painting() {
	for stream in [
		&b"\"1;1;2;2!5~"[..],
		b"\"1;1;30;20~",
		b"\"2;1;3;3~-~",
		b"\"1;1;0;0",
	] {
		same_as_reference(stream, 2, 0x112233, 0x445566);
	}
	let image = same_as_reference(b"\"1;1;30;20~", 2, 0x112233, 0x445566).unwrap();
	assert_eq!((image.width, image.height), (30, 20));
	assert!(image.opaque);
}

#[test]
fn sizes_beyond_the_limits_are_refused() {
	let repeat = |count: usize, data: char| format!("!{count}{data}").into_bytes();
	let tall = "-".repeat(MAX_SIDE / 6 + 1) + "~";
	let refused = [
		repeat(MAX_SIDE + 1, '~'),
		repeat(usize::MAX, '?'),
		format!("!{}0~", "9".repeat(30)).into_bytes(),
		b"\"1;1;4097;1~".to_vec(),
		b"\"1;1;4096;4097~".to_vec(),
		tall.into_bytes(),
		// Painting more than the budget, a column at a time.
		format!("\"10;1{}", "!4096~$".repeat(300)).into_bytes(),
	];
	for stream in refused {
		assert!(same_as_reference(&stream, 2, TRANSPARENT, 0).is_none());
	}
	// The largest image allowed still decodes.
	let image = same_as_reference(b"\"1;1;4096;4092!4096~", 1, TRANSPARENT, 0).unwrap();
	assert_eq!((image.width, image.height), (4096, 4092));
}

#[test]
fn an_aspect_change_resizes_later_bands_only() {
	let image = same_as_reference(b"#16~-\"3;1~$\"1;1!2@", 1, TRANSPARENT, 0x00ff00).unwrap();
	assert_eq!((image.width, image.height), (2, 24));
	assert_eq!(image.pixels[(6 + 2) * 2], 0xff00ff00);
}

#[test]
fn a_foreground_equal_to_the_transparent_marker_stays_transparent() {
	// Colors from 16 on start as the foreground.
	same_as_reference(b"#16~!3@#3~", 2, 0x10, TRANSPARENT);
	let image = same_as_reference(b"#16~", 1, 0x10, TRANSPARENT).unwrap();
	assert_eq!(image.pixels, [0; 6]);
}
