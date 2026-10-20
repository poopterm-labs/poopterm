//! The original per-row Sixel decoder, kept as the oracle the flat
//! [`super::Decoder`] is tested against.
use super::{MAX_PIXELS, MAX_SIDE, TRANSPARENT, hls, number, rgb};
use crate::graphics::Bitmap;

pub(super) struct Decoder {
	rows: Vec<Vec<u32>>,
	width: usize,
	height: usize,
	x: usize,
	y: usize,
	aspect: usize,
	background: u32,
	colors: [u32; 256],
	selected: usize,
	painted: usize,
}
impl Decoder {
	pub(super) fn new(aspect: usize, background: u32, foreground: u32) -> Self {
		let mut colors = [foreground; 256];
		for (index, color) in colors.iter_mut().enumerate().take(16) {
			*color = [
				0, 0x3333cc, 0xcc2121, 0x33cc33, 0xcc33cc, 0x33cccc, 0xcccc33, 0x878787, 0x424242,
				0x545499, 0x995454, 0x549954, 0x995499, 0x549999, 0x999954, 0xcccccc,
			][index];
		}
		Self {
			rows: Vec::new(),
			width: 0,
			height: 0,
			x: 0,
			y: 0,
			aspect,
			background,
			colors,
			selected: 0,
			painted: 0,
		}
	}
	fn size(&mut self, width: usize, height: usize) -> Option<()> {
		let width = self.width.max(width);
		let height = self.height.max(height);
		if width > MAX_SIDE || height > MAX_SIDE || width.checked_mul(height)? > MAX_PIXELS {
			return None;
		}
		self.width = width;
		self.height = height;
		self.rows.resize_with(height, Vec::new);
		Some(())
	}
	fn paint(&mut self, byte: u8, repeat: usize) -> Option<()> {
		self.painted = self
			.painted
			.checked_add(repeat.checked_mul(6 * self.aspect)?)?;
		if self.painted > 64 * 1024 * 1024 {
			return None;
		}
		self.size(
			self.x.checked_add(repeat)?,
			self.y.checked_add(6 * self.aspect)?,
		)?;
		for bit in 0..6 {
			if (byte - b'?') & (1 << bit) == 0 {
				continue;
			}
			for dy in 0..self.aspect {
				let row_idx = self.y + bit * self.aspect + dy;
				if row_idx < self.rows.len() {
					let row = &mut self.rows[row_idx];
					row.resize(self.width, self.background);
					row[self.x..self.x + repeat].fill(self.colors[self.selected]);
				}
			}
		}
		self.x += repeat;
		Some(())
	}
	pub(super) fn decode(mut self, bytes: &[u8]) -> Option<Bitmap> {
		let mut i = 0;
		while i < bytes.len() {
			let byte = bytes[i];
			i += 1;
			match byte {
				b'?'..=b'~' => self.paint(byte, 1)?,
				b'!' => {
					let count = number(bytes, &mut i).max(1);
					let byte = *bytes.get(i)?;
					i += 1;
					if !(b'?'..=b'~').contains(&byte) {
						return None;
					}
					self.paint(byte, count)?;
				}
				b'$' => self.x = 0,
				b'-' => {
					self.x = 0;
					self.y = self.y.checked_add(6 * self.aspect)?;
					if self.y > MAX_SIDE {
						return None;
					}
				}
				b'#' | b'"' => {
					let mut params = vec![number(bytes, &mut i)];
					while bytes.get(i) == Some(&b';') {
						i += 1;
						if params.len() == 6 {
							return None;
						}
						params.push(number(bytes, &mut i));
					}
					if byte == b'#' {
						self.selected = params[0];
						if self.selected >= self.colors.len() {
							return None;
						}
						if params.len() >= 5 {
							self.colors[self.selected] = match params[1] {
								2 => rgb(params[2], params[3], params[4]),
								1 => hls(params[2], params[3], params[4]),
								_ => self.colors[self.selected],
							};
						}
					} else if params.len() >= 2 {
						let numerator = params[0].max(1);
						let denominator = params[1].max(1);
						self.aspect = numerator
							.saturating_add(denominator / 2)
							.checked_div(denominator)?
							.clamp(1, 10);
						if params.len() >= 4 {
							self.size(params[2], params[3].checked_mul(self.aspect)?)?;
						}
					}
				}
				0x00..=0x20 => {}
				_ => {}
			}
		}
		if self.width == 0 || self.height == 0 {
			return None;
		}
		let mut pixels = vec![self.background; self.width * self.height];
		for (y, row) in self.rows.iter().enumerate() {
			if y < self.height && !row.is_empty() {
				let copy_len = row.len().min(self.width);
				pixels[y * self.width..y * self.width + copy_len].copy_from_slice(&row[..copy_len]);
			}
		}
		for pixel in &mut pixels {
			*pixel = if *pixel == TRANSPARENT {
				0
			} else {
				*pixel | 0xff00_0000
			};
		}
		Some(Bitmap::new(self.width, self.height, pixels))
	}
}
