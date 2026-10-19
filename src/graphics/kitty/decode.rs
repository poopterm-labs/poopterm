//! Turns transmitted image data (`f=24` RGB, `f=32` RGBA, `f=100` PNG,
//! optionally zlib-compressed with `o=z`) into a [`Bitmap`].
use std::io::Cursor;

use super::{Command, Error, MAX_TRANSFER};
use crate::graphics::Bitmap;

/// Largest width or height accepted.
pub(crate) const MAX_SIDE: usize = 16384;
/// Largest image in pixels (256 MiB decoded).
pub(crate) const MAX_PIXELS: usize = 1 << 26;

pub(crate) fn decode(command: &Command, data: &[u8]) -> Result<Bitmap, Error> {
	let format = command.uint(b'f');
	let format = if format == 0 { 32 } else { format };
	let compressed = match command.letter(b'o', 0) {
		0 => false,
		b'z' => true,
		_ => return Err(Error::new("EINVAL", "unknown compression")),
	};
	match format {
		24 | 32 => {
			let (width, height) = (command.uint(b's') as usize, command.uint(b'v') as usize);
			if width == 0 || height == 0 {
				return Err(Error::new("EINVAL", "raw pixel data needs s and v"));
			}
			check_size(width, height)?;
			let depth = if format == 24 { 3 } else { 4 };
			let size = width * height * depth;
			let inflated;
			let data = if compressed {
				inflated = inflate(data, size)?;
				&inflated[..]
			} else {
				data
			};
			let data = data
				.get(..size)
				.ok_or_else(|| Error::new("ENODATA", "insufficient image data"))?;
			let pixels = if depth == 3 {
				from_rgb(data)
			} else {
				from_rgba(data)
			};
			Ok(Bitmap::new(width, height, pixels))
		}
		100 => {
			let inflated;
			let data = if compressed {
				inflated = inflate(data, MAX_TRANSFER)?;
				&inflated[..]
			} else {
				data
			};
			decode_png(data)
		}
		_ => Err(Error::new("EINVAL", "unknown image format")),
	}
}

fn check_size(width: usize, height: usize) -> Result<(), Error> {
	if width > MAX_SIDE || height > MAX_SIDE || width * height > MAX_PIXELS {
		return Err(Error::new("EFBIG", "image too large"));
	}
	Ok(())
}

/// Inflates zlib data, refusing output larger than `limit`.
fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, Error> {
	miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, limit)
		.map_err(|_| Error::new("EINVAL", "bad or oversized zlib data"))
}

fn from_rgb(data: &[u8]) -> Vec<u32> {
	let (pixels, _) = data.as_chunks::<3>();
	pixels
		.iter()
		.map(|&[r, g, b]| u32::from_be_bytes([0xff, r, g, b]))
		.collect()
}

fn from_rgba(data: &[u8]) -> Vec<u32> {
	let (pixels, _) = data.as_chunks::<4>();
	pixels
		.iter()
		.map(|&pixel| {
			// Little-endian RGBA is 0xAABBGGRR: swap red and blue.
			let value = u32::from_le_bytes(pixel);
			value & 0xff00_ff00 | (value >> 16) & 0xff | (value & 0xff) << 16
		})
		.collect()
}

fn decode_png(data: &[u8]) -> Result<Bitmap, Error> {
	let bad = |error: image::ImageError| Error::new("EBADPNG", error.to_string());
	let mut reader = image::ImageReader::with_format(Cursor::new(data), image::ImageFormat::Png);
	let mut limits = image::Limits::default();
	limits.max_image_width = Some(MAX_SIDE as u32);
	limits.max_image_height = Some(MAX_SIDE as u32);
	// Room for one 16-bit RGBA image of the largest size accepted.
	limits.max_alloc = Some(MAX_PIXELS as u64 * 8);
	reader.limits(limits);
	let image = reader.decode().map_err(bad)?.into_rgba8();
	let (width, height) = (image.width() as usize, image.height() as usize);
	check_size(width, height)?;
	Ok(Bitmap::new(width, height, from_rgba(image.as_raw())))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn command(control: &str) -> Command {
		Command::parse(control.as_bytes()).unwrap()
	}

	fn code(result: Result<Bitmap, Error>) -> &'static str {
		result.unwrap_err().code
	}

	#[test]
	fn raw_rgb_and_rgba_become_argb() {
		let rgb = decode(&command("f=24,s=2,v=1"), &[1, 2, 3, 4, 5, 6]).unwrap();
		assert_eq!(rgb.pixels, [0xff010203, 0xff040506]);
		assert!(rgb.opaque);
		let rgba = decode(&command("s=1,v=2"), &[1, 2, 3, 0x80, 4, 5, 6, 255]).unwrap();
		assert_eq!((rgba.width, rgba.height), (1, 2));
		assert_eq!(rgba.pixels, [0x80010203, 0xff040506]);
		assert!(!rgba.opaque);
	}

	#[test]
	fn raw_data_must_match_the_size() {
		assert_eq!(
			code(decode(&command("f=24,s=2,v=1"), &[1, 2, 3])),
			"ENODATA"
		);
		assert_eq!(code(decode(&command("f=32,v=1"), &[1, 2, 3, 4])), "EINVAL");
		assert_eq!(code(decode(&command("f=7,s=1,v=1"), &[0; 4])), "EINVAL");
		assert_eq!(code(decode(&command("f=32,s=100000,v=1"), &[])), "EFBIG");
		assert_eq!(code(decode(&command("f=32,s=10000,v=10000"), &[])), "EFBIG");
	}

	#[test]
	fn zlib_compressed_rgb_is_inflated_within_the_expected_size() {
		let rgb: Vec<u8> = (0..4 * 3 * 3).map(|i| i as u8).collect();
		let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
		let bitmap = decode(&command("f=24,o=z,s=4,v=3"), &compressed).unwrap();
		assert_eq!(bitmap.pixels[1], 0xff030405);
		// Inflating to more than the image needs is refused, not buffered.
		let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&[0; 1 << 20], 6);
		assert_eq!(code(decode(&command("f=24,o=z,s=4,v=3"), &bomb)), "EINVAL");
		assert_eq!(
			code(decode(&command("f=24,o=z,s=4,v=3"), b"junk")),
			"EINVAL"
		);
	}

	#[test]
	fn png_is_decoded_with_its_own_size() {
		use image::ImageEncoder;
		let mut png = Vec::new();
		image::codecs::png::PngEncoder::new(&mut png)
			.write_image(
				&[9, 8, 7, 255, 1, 2, 3, 0],
				2,
				1,
				image::ExtendedColorType::Rgba8,
			)
			.unwrap();
		let bitmap = decode(&command("f=100"), &png).unwrap();
		assert_eq!(bitmap.pixels, [0xff090807, 0x00010203]);
		let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&png, 6);
		assert_eq!(decode(&command("f=100,o=z"), &compressed).unwrap(), bitmap);
		assert_eq!(code(decode(&command("f=100"), b"not a png")), "EBADPNG");
	}
}
