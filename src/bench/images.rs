//! Image workloads: Sixel and the kitty graphics protocol, as image viewers
//! (timg, chafa, mpv's kitty output) send them.
use super::corpus::{Rng, Size};

/// RGBA pixels of a `width`×`height` gradient that changes with `phase`.
fn gradient(width: usize, height: usize, phase: usize) -> Vec<u8> {
	let mut pixels = Vec::with_capacity(width * height * 4);
	for y in 0..height {
		for x in 0..width {
			let r = (x * 255 / width.max(1) + phase) as u8;
			let g = (y * 255 / height.max(1)) as u8;
			let b = ((x ^ y) + phase) as u8;
			pixels.extend_from_slice(&[r, g, b, 255]);
		}
	}
	pixels
}

/// Moves to a random cell where an image of `rows` lines fits.
fn place_cursor(rng: &mut Rng, size: Size, rows: usize, out: &mut Vec<u8>) {
	let row = 1 + rng.below(size.rows.saturating_sub(rows).max(1));
	let col = 1 + rng.below(size.cols / 2);
	out.extend_from_slice(format!("\x1b[{row};{col}H").as_bytes());
}

pub(super) fn sixel(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	let (width, height) = (*rng.pick(&[64usize, 128, 256]), 96usize);
	let phase = rng.below(256);
	place_cursor(rng, size, 6, out);
	// P2=1: transparent background; raster attributes give the size.
	out.extend_from_slice(format!("\x1bP0;1;0q\"1;1;{width};{height}").as_bytes());
	for color in 0..16 {
		let level = |shift: usize| (color * 6 + shift + phase) % 101;
		out
			.extend_from_slice(format!("#{color};2;{};{};{}", level(0), level(40), level(70)).as_bytes());
	}
	for band in 0..height / 6 {
		for color in 0..16 {
			out.extend_from_slice(format!("#{color}").as_bytes());
			let mut x = 0;
			while x < width {
				let run = (1 + rng.below(24)).min(width - x);
				// Each color paints some rows of the band, by position and color.
				let bits = ((band + x / 8 + color) % 63) as u8 + 1;
				let char = if (x / 8 + color) % 4 == 0 { bits } else { 0 };
				if run > 3 {
					out.extend_from_slice(format!("!{run}").as_bytes());
					out.push(0x3f + char);
				} else {
					out.extend(std::iter::repeat_n(0x3f + char, run));
				}
				x += run;
			}
			out.push(b'$');
		}
		out.push(b'-');
	}
	out.extend_from_slice(b"\x1b\\");
}

/// Standard base64 with padding, as the kitty protocol expects.
pub(crate) fn base64(data: &[u8]) -> Vec<u8> {
	const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
	for chunk in data.chunks(3) {
		let n = u32::from(chunk[0]) << 16
			| u32::from(*chunk.get(1).unwrap_or(&0)) << 8
			| u32::from(*chunk.get(2).unwrap_or(&0));
		for i in 0..4 {
			out.push(if i <= chunk.len() {
				TABLE[(n >> (18 - 6 * i)) as usize & 63]
			} else {
				b'='
			});
		}
	}
	out
}

/// Sends `payload` as one kitty graphics transmission in 4096-byte chunks
/// (the protocol's maximum), with `control` keys on the first chunk.
fn kitty_transmit(control: &str, payload: &[u8], out: &mut Vec<u8>) {
	let encoded = base64(payload);
	let mut chunks = encoded.chunks(4096).peekable();
	let mut first = true;
	while let Some(chunk) = chunks.next() {
		let more = u8::from(chunks.peek().is_some());
		if first {
			out.extend_from_slice(format!("\x1b_G{control},m={more};").as_bytes());
			first = false;
		} else {
			out.extend_from_slice(format!("\x1b_Gm={more};").as_bytes());
		}
		out.extend_from_slice(chunk);
		out.extend_from_slice(b"\x1b\\");
	}
}

/// A random image size and id; ids repeat so memory stays bounded.
fn kitty_image(rng: &mut Rng) -> (usize, usize, usize) {
	let side = *rng.pick(&[32usize, 64, 128, 256]);
	(side, side * 3 / 4, 1 + rng.below(8))
}

pub(super) fn kitty_rgba(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	let (width, height, id) = kitty_image(rng);
	place_cursor(rng, size, 4, out);
	let pixels = gradient(width, height, rng.below(256));
	// q=2 suppresses replies; C=1 keeps the cursor where it is.
	let control = format!("a=T,f=32,s={width},v={height},i={id},q=2,C=1");
	kitty_transmit(&control, &pixels, out);
}

pub(super) fn kitty_png(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	use image::ImageEncoder;
	let (width, height, id) = kitty_image(rng);
	place_cursor(rng, size, 4, out);
	let pixels = gradient(width, height, rng.below(256));
	let mut png = Vec::new();
	image::codecs::png::PngEncoder::new(&mut png)
		.write_image(
			&pixels,
			width as u32,
			height as u32,
			image::ExtendedColorType::Rgba8,
		)
		.expect("in-memory PNG encoding cannot fail");
	kitty_transmit(&format!("a=T,f=100,i={id},q=2,C=1"), &png, out);
}

pub(super) fn kitty_zlib(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	let (width, height, id) = kitty_image(rng);
	place_cursor(rng, size, 4, out);
	let rgb: Vec<u8> = gradient(width, height, rng.below(256))
		.chunks(4)
		.flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
		.collect();
	let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
	let control = format!("a=T,f=24,o=z,s={width},v={height},i={id},q=2,C=1");
	kitty_transmit(&control, &compressed, out);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn base64_matches_the_rfc_4648_vectors() {
		for (input, output) in [
			("", ""),
			("f", "Zg=="),
			("fo", "Zm8="),
			("foo", "Zm9v"),
			("foobar", "Zm9vYmFy"),
		] {
			assert_eq!(base64(input.as_bytes()), output.as_bytes());
		}
	}
}
