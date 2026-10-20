//! Differential tests of the PNG decoder against the `image` crate, on
//! images from a reference encoder written from the specification: every
//! color type, bit depth, filter and interlace method.
use super::{Error, decode};
use crate::bench::corpus::Rng;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// The Adam7 passes: first column and row, then column and row steps.
const PASSES: [(usize, usize, usize, usize); 7] = [
	(0, 0, 8, 8),
	(4, 0, 8, 8),
	(0, 4, 4, 8),
	(2, 0, 4, 4),
	(0, 2, 2, 4),
	(1, 0, 2, 2),
	(0, 1, 1, 2),
];
/// Every valid color type with its bit depths.
const KINDS: [(u8, &[u8]); 5] = [
	(0, &[1, 2, 4, 8, 16]),
	(2, &[8, 16]),
	(3, &[1, 2, 4, 8]),
	(4, &[8, 16]),
	(6, &[8, 16]),
];

/// CRC-32 bit by bit, as the specification defines it.
pub(super) fn reference_crc(bytes: &[u8]) -> u32 {
	let mut crc = !0u32;
	for &byte in bytes {
		crc ^= u32::from(byte);
		for _ in 0..8 {
			crc = if crc & 1 != 0 {
				crc >> 1 ^ 0xedb8_8320
			} else {
				crc >> 1
			};
		}
	}
	!crc
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
	let p = i16::from(a) + i16::from(b) - i16::from(c);
	let (pa, pb, pc) = (
		(p - i16::from(a)).abs(),
		(p - i16::from(b)).abs(),
		(p - i16::from(c)).abs(),
	);
	if pa <= pb && pa <= pc {
		a
	} else if pb <= pc {
		b
	} else {
		c
	}
}

/// `row` filtered with `filter` against `prior`, the raw row above, with
/// `bpp` bytes per complete pixel.
pub(super) fn filter(filter: u8, row: &[u8], prior: &[u8], bpp: usize) -> Vec<u8> {
	(0..row.len())
		.map(|i| {
			let a = if i >= bpp { row[i - bpp] } else { 0 };
			let (b, c) = (prior[i], if i >= bpp { prior[i - bpp] } else { 0 });
			let predicted = match filter {
				0 => 0,
				1 => a,
				2 => b,
				3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
				_ => paeth(a, b, c),
			};
			row[i].wrapping_sub(predicted)
		})
		.collect()
}

pub(super) fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
	png.extend((data.len() as u32).to_be_bytes());
	let start = png.len();
	png.extend(kind);
	png.extend(data);
	let crc = reference_crc(&png[start..]);
	png.extend(crc.to_be_bytes());
}

fn bit(row: &[u8], i: usize) -> bool {
	row[i / 8] >> (7 - i % 8) & 1 != 0
}

/// An image to encode, with its raw rows packed as the specification lays
/// them out without interlacing.
#[derive(Clone)]
pub(super) struct Png {
	pub width: usize,
	pub height: usize,
	pub depth: u8,
	pub color: u8,
	pub interlace: bool,
	pub palette: Vec<u8>,
	pub transparency: Option<Vec<u8>>,
	/// The filter of each scanline, cycled.
	pub filters: Vec<u8>,
	pub rows: Vec<u8>,
	/// The most compressed bytes in one IDAT chunk.
	pub idat: usize,
}

impl Png {
	/// Random pixels, palette and filters.
	pub fn random(rng: &mut Rng, width: usize, height: usize, color: u8, depth: u8) -> Self {
		let mut png = Self {
			width,
			height,
			depth,
			color,
			interlace: false,
			palette: Vec::new(),
			transparency: None,
			filters: (0..7).map(|_| rng.below(5) as u8).collect(),
			rows: Vec::new(),
			idat: 1 + rng.below(300),
		};
		png.rows = (0..png.row_bytes(width) * height)
			.map(|_| rng.next() as u8)
			.collect();
		if color == 3 {
			// One entry per possible index, so every pixel is valid.
			png.palette = (0..3 << depth).map(|_| rng.next() as u8).collect();
		}
		png
	}

	fn bits(&self) -> usize {
		let channels = match self.color {
			0 | 3 => 1,
			2 => 3,
			4 => 2,
			_ => 4,
		};
		channels * usize::from(self.depth)
	}

	fn row_bytes(&self, width: usize) -> usize {
		(width * self.bits()).div_ceil(8)
	}

	/// The filtered scanlines of every pass, before compression.
	pub fn scanlines(&self) -> Vec<u8> {
		let passes = if self.interlace {
			PASSES.to_vec()
		} else {
			vec![(0, 0, 1, 1)]
		};
		let (bits, full, bpp) = (self.bits(), self.row_bytes(self.width), self.bits().div_ceil(8));
		let (mut data, mut line) = (Vec::new(), 0);
		for (x0, y0, dx, dy) in passes {
			let width = self.width.saturating_sub(x0).div_ceil(dx);
			let height = self.height.saturating_sub(y0).div_ceil(dy);
			if width == 0 || height == 0 {
				continue;
			}
			let mut prior = vec![0; self.row_bytes(width)];
			for y in 0..height {
				let source = &self.rows[(y0 + y * dy) * full..][..full];
				let mut row = vec![0; self.row_bytes(width)];
				if dx == 1 {
					row.copy_from_slice(source);
				}
				for i in (0..width * bits).filter(|_| dx > 1) {
					if bit(source, (x0 + i / bits * dx) * bits + i % bits) {
						row[i / 8] |= 0x80 >> (i % 8);
					}
				}
				let kind = self.filters[line % self.filters.len()];
				line += 1;
				data.push(kind);
				data.extend(filter(kind, &row, &prior, bpp));
				prior = row;
			}
		}
		data
	}

	pub fn header(&self) -> Vec<u8> {
		let mut header = Vec::new();
		header.extend((self.width as u32).to_be_bytes());
		header.extend((self.height as u32).to_be_bytes());
		header.extend([self.depth, self.color, 0, 0, u8::from(self.interlace)]);
		header
	}

	/// The PNG file with `extra` chunks before the image data.
	pub fn encode_with(&self, extra: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
		let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&self.scanlines(), 6);
		let mut png = SIGNATURE.to_vec();
		chunk(&mut png, b"IHDR", &self.header());
		for (kind, data) in extra {
			chunk(&mut png, kind, data);
		}
		if !self.palette.is_empty() {
			chunk(&mut png, b"PLTE", &self.palette);
		}
		if let Some(transparency) = &self.transparency {
			chunk(&mut png, b"tRNS", transparency);
		}
		for piece in zlib.chunks(self.idat) {
			chunk(&mut png, b"IDAT", piece);
		}
		chunk(&mut png, b"IEND", &[]);
		png
	}

	pub fn encode(&self) -> Vec<u8> {
		self.encode_with(&[])
	}
}

fn ours(png: &[u8]) -> Result<Vec<u32>, Error> {
	decode(png, 1 << 14, 1 << 26).map(|bitmap| bitmap.pixels)
}

/// The `image` crate's pixels as ARGB, or `None` if it refuses the file.
fn oracle(png: &[u8]) -> Option<Vec<u32>> {
	let image = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
	let (pixels, _) = image.into_rgba8().into_raw().as_chunks::<4>().to_owned();
	Some(
		pixels
			.iter()
			.map(|&[r, g, b, a]| u32::from_be_bytes([a, r, g, b]))
			.collect(),
	)
}

/// Decodes `png` both ways, expecting both to succeed with the same pixels.
fn agrees(png: &[u8]) {
	let expected = oracle(png).expect("the reference decodes it");
	assert_eq!(ours(png), Ok(expected));
}

#[test]
fn every_color_type_depth_and_size_matches_the_reference() {
	let mut rng = Rng::new(0x9e7);
	let sizes = [(1, 1), (2, 3), (5, 1), (7, 9), (8, 8), (13, 5), (33, 17)];
	for (color, depths) in KINDS {
		for &depth in depths {
			for interlace in [false, true] {
				for (width, height) in sizes {
					let mut png = Png::random(&mut rng, width, height, color, depth);
					png.interlace = interlace;
					agrees(&png.encode());
				}
			}
		}
	}
}

#[test]
fn every_filter_type_is_undone_on_every_row() {
	let mut rng = Rng::new(0xf17);
	for (color, depths) in KINDS {
		for &depth in depths {
			for kind in 0..5 {
				let mut png = Png::random(&mut rng, 11, 6, color, depth);
				png.filters = vec![kind];
				agrees(&png.encode());
			}
		}
	}
}

#[test]
fn transparency_keys_and_palette_alpha_match_the_reference() {
	let mut rng = Rng::new(0x7e5);
	for (color, depths) in [KINDS[0], KINDS[1], KINDS[2]] {
		for &depth in depths {
			for interlace in [false, true] {
				let mut png = Png::random(&mut rng, 9, 7, color, depth);
				png.interlace = interlace;
				let key = match color {
					// The first pixel's sample (or samples), widened to 16 bits.
					0 => {
						let bits = usize::from(depth);
						let first = u16::from_be_bytes([png.rows[0], png.rows[1 % png.rows.len()]]);
						(first >> (16 - bits)).to_be_bytes().to_vec()
					}
					2 if depth == 8 => png.rows[..3].iter().flat_map(|&v| [0, v]).collect(),
					2 => png.rows[..6].to_vec(),
					// Alpha for some of the palette, the rest opaque.
					_ => (0..png.palette.len() / 3 / 2).map(|_| rng.next() as u8).collect(),
				};
				png.transparency = Some(key);
				agrees(&png.encode());
			}
		}
	}
}

#[test]
fn sixteen_bit_samples_narrow_like_the_reference() {
	// Every 16-bit gray value once.
	let mut png = Png::random(&mut Rng::new(1), 256, 256, 0, 16);
	png.rows = (0..=u16::MAX).flat_map(u16::to_be_bytes).collect();
	agrees(&png.encode());
	let mut rgba = Png::random(&mut Rng::new(2), 128, 128, 6, 16);
	rgba.rows = (0..=u16::MAX).flat_map(u16::to_be_bytes).collect();
	agrees(&rgba.encode());
}

#[test]
fn ancillary_and_empty_chunks_are_skipped() {
	let mut png = Png::random(&mut Rng::new(3), 10, 10, 6, 8);
	png.idat = 7;
	let file = png.encode_with(&[(b"tEXt", b"Title\0x"), (b"prVt", &[1, 2, 3]), (b"IDAT", &[])]);
	agrees(&file);
	// An empty chunk among the image data, and none at all after it.
	let mut png = Png::random(&mut Rng::new(4), 6, 4, 2, 8);
	png.idat = 1 << 20;
	let mut file = png.encode_with(&[(b"IDAT", &[])]);
	file.truncate(file.len() - 12);
	agrees(&file);
}

#[test]
fn chunks_after_the_image_data_are_not_read() {
	let png = Png::random(&mut Rng::new(5), 4, 4, 6, 8);
	let mut file = png.encode();
	// A damaged end chunk, and garbage after it.
	let end = file.len() - 1;
	file[end] ^= 1;
	file.extend(b"garbage");
	agrees(&file);
}

#[test]
fn the_benchmark_images_match_the_reference() {
	use image::ImageEncoder;
	let mut rng = Rng::new(6);
	for (width, height) in [(1, 1), (64, 48), (301, 7)] {
		let pixels: Vec<u8> = (0..width * height * 4).map(|_| rng.next() as u8).collect();
		let gradient: Vec<u8> = (0..width * height * 4).map(|i| (i / 7 % 251) as u8).collect();
		for pixels in [pixels, gradient] {
			let mut file = Vec::new();
			image::codecs::png::PngEncoder::new(&mut file)
				.write_image(&pixels, width as u32, height as u32, image::ExtendedColorType::Rgba8)
				.unwrap();
			agrees(&file);
		}
	}
}

/// `png` with its first chunk of `kind` replaced by `data`, CRC fixed.
fn with_chunk(png: &[u8], kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
	let mut at = 8;
	while &png[at + 4..at + 8] != kind {
		at += 12 + u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
	}
	let end = at + 12 + u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
	let mut out = png[..at].to_vec();
	chunk(&mut out, kind, data);
	out.extend(&png[end..]);
	out
}

fn invalid(png: &[u8]) -> bool {
	matches!(ours(png), Err(Error::Invalid(_)))
}

#[test]
fn invalid_headers_are_refused() {
	let png = Png::random(&mut Rng::new(7), 3, 3, 2, 8);
	let file = png.encode();
	let header = png.header();
	let changed = |at: usize, value: u8| {
		let mut header = header.clone();
		header[at] = value;
		with_chunk(&file, b"IHDR", &header)
	};
	// Depths each color type does not allow, and unknown color types.
	for (color, depth) in [(0, 3), (0, 32), (2, 4), (3, 16), (4, 1), (6, 2), (1, 8), (5, 8), (7, 8)] {
		assert!(invalid(&changed(9, color).iter().copied().collect::<Vec<_>>()) || color == 2);
		let mut header = header.clone();
		header[8..10].copy_from_slice(&[depth, color]);
		assert!(invalid(&with_chunk(&file, b"IHDR", &header)), "{color}/{depth}");
	}
	// Unknown compression, filter and interlace methods.
	for (at, value) in [(10, 1), (11, 1), (12, 2)] {
		assert!(invalid(&changed(at, value)), "{at}");
	}
	// Zero and out-of-range sizes.
	for at in [0, 4] {
		let mut header = header.clone();
		header[at..at + 4].copy_from_slice(&0u32.to_be_bytes());
		assert!(invalid(&with_chunk(&file, b"IHDR", &header)));
		header[at..at + 4].copy_from_slice(&(1u32 << 31).to_be_bytes());
		assert!(ours(&with_chunk(&file, b"IHDR", &header)).is_err());
	}
	assert!(invalid(&with_chunk(&file, b"IHDR", &header[..12])));
}

#[test]
fn damaged_files_are_refused() {
	let png = Png::random(&mut Rng::new(8), 5, 5, 6, 8);
	let file = png.encode();
	assert!(invalid(b""));
	assert!(invalid(&file[..8]));
	assert!(invalid(&file[1..]));
	// A damaged CRC or chunk body before or in the image data.
	for at in [8 + 4 + 4 + 13, 8 + 8, 33 + 8 + 2, 33 + 8 + 1] {
		let mut damaged = file.clone();
		damaged[at] ^= 0x10;
		assert!(invalid(&damaged), "{at}");
	}
	// Truncated in the image data, and a chunk claiming more than is there.
	for cut in [40, 50, file.len() - 13] {
		assert!(invalid(&file[..cut]), "{cut}");
	}
	let mut long = file.clone();
	long[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
	assert!(invalid(&long));
	// The image data must come after the header.
	let mut swapped = SIGNATURE.to_vec();
	chunk(&mut swapped, b"IDAT", &[]);
	swapped.extend(&file[8..]);
	assert!(invalid(&swapped));
}

#[test]
fn bad_image_data_is_refused() {
	let mut rng = Rng::new(9);
	let png = Png::random(&mut rng, 6, 4, 6, 8);
	let encode = |scanlines: &[u8]| {
		let zlib = miniz_oxide::deflate::compress_to_vec_zlib(scanlines, 6);
		with_chunk(&png.encode(), b"IDAT", &zlib)
	};
	let good = png.scanlines();
	assert_eq!(ours(&encode(&good)), Ok(oracle(&encode(&good)).unwrap()));
	// One byte short, one byte more, and an unknown filter type.
	assert!(invalid(&encode(&good[1..])));
	let mut more = good.clone();
	more.push(0);
	assert!(invalid(&encode(&more)));
	let mut unknown = good.clone();
	unknown[25 * 3] = 5;
	assert!(invalid(&encode(&unknown)));
	// Not zlib at all, and a damaged checksum.
	assert!(invalid(&with_chunk(&png.encode(), b"IDAT", b"junk")));
	let mut zlib = miniz_oxide::deflate::compress_to_vec_zlib(&good, 6);
	*zlib.last_mut().unwrap() ^= 1;
	assert!(invalid(&with_chunk(&png.encode(), b"IDAT", &zlib)));
}

#[test]
fn palettes_are_required_bounded_and_complete() {
	let mut rng = Rng::new(10);
	let png = Png::random(&mut rng, 4, 4, 3, 8);
	let file = png.encode();
	// Missing, empty, not whole entries, and more than 256 entries.
	let mut missing = png.clone();
	missing.palette.clear();
	assert!(invalid(&missing.encode()));
	for size in [0, 4, 257 * 3] {
		assert!(invalid(&with_chunk(&file, b"PLTE", &vec![7; size])), "{size}");
	}
	// An index past the end of the palette.
	let mut short = png.clone();
	short.palette.truncate(3);
	short.rows = vec![0, 1, 0, 0].repeat(4);
	assert_eq!(ours(&short.encode()).is_ok(), oracle(&short.encode()).is_some());
	// Gray images may not carry one; color images may, and ignore it.
	let mut gray = Png::random(&mut rng, 4, 4, 0, 8);
	gray.palette = vec![1, 2, 3];
	assert!(invalid(&gray.encode()));
	let mut rgb = Png::random(&mut rng, 4, 4, 2, 8);
	rgb.palette = vec![1, 2, 3];
	agrees(&rgb.encode());
}

#[test]
fn unknown_critical_chunks_are_refused() {
	let png = Png::random(&mut Rng::new(11), 3, 3, 6, 8);
	assert!(invalid(&png.encode_with(&[(b"CRIT", &[1])])));
	agrees(&png.encode_with(&[(b"cRIT", &[1])]));
}

#[test]
fn oversized_images_are_refused_before_inflating() {
	let mut png = Png::random(&mut Rng::new(12), 1, 1, 6, 8);
	let file = png.encode();
	for (width, height) in [(16385, 1), (1, 16385), (16384, 4097)] {
		png.width = width;
		png.height = height;
		// No image data for that size: the header alone decides.
		assert_eq!(ours(&with_chunk(&file, b"IHDR", &png.header())), Err(Error::TooLarge));
	}
	png.width = 16384;
	png.height = 1;
	png.rows = vec![0; 16384 * 4];
	assert_eq!(ours(&png.encode()).map(|pixels| pixels.len()), Ok(16384));
	assert_eq!(decode(&file, 1, 0).map(|bitmap| bitmap.width), Err(Error::TooLarge));
}

#[test]
fn opacity_is_reported_for_the_bitmap() {
	let mut png = Png::random(&mut Rng::new(13), 3, 2, 2, 8);
	assert!(decode(&png.encode(), 64, 64).unwrap().opaque);
	png.transparency = Some(png.rows[..3].iter().flat_map(|&v| [0, v]).collect());
	let bitmap = decode(&png.encode(), 64, 64).unwrap();
	assert!(!bitmap.opaque);
	assert_eq!(bitmap.pixels[0] >> 24, 0);
	assert_eq!((bitmap.width, bitmap.height), (3, 2));
}
