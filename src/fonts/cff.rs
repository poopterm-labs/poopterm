//! Compact Font Format outlines (`CFF ` and `CFF2` tables): INDEX and DICT
//! structures, CID font selection, and the Type 2 charstring interpreter.
//! Variable CFF2 fonts are drawn at their default instance.
use super::{
	face::{Bounds, Error, Sink, Table},
	read::Bytes,
};

/// The argument stack limit: 48 in CFF, raised to at most 513 by CFF2.
const MAX_STACK: usize = 513;
/// The Type 2 subroutine nesting limit.
const MAX_CALL_DEPTH: usize = 10;
/// Operators one glyph may execute; bounds hostile subroutine trees.
const MAX_OPERATORS: usize = 100_000;
const MALFORMED: Error = "malformed CFF table";

/// An INDEX: `count` byte strings, located by offsets relative to `base`.
#[derive(Clone, Copy, Debug, Default)]
struct Index {
	count: usize,
	off_size: usize,
	offsets: usize,
	base: usize,
	end: usize,
}

impl Index {
	/// The INDEX at `at`; CFF2 (`wide`) counts are 32-bit.
	fn parse(cff: &[u8], at: usize, wide: bool) -> Option<Self> {
		let (count, at) = if wide {
			(cff.u32_at(at)? as usize, at + 4)
		} else {
			(usize::from(cff.u16_at(at)?), at + 2)
		};
		if count == 0 {
			return Some(Self {
				end: at,
				..Self::default()
			});
		}
		let off_size = usize::from(cff.u8_at(at)?);
		if !(1..=4).contains(&off_size) || count > cff.len() {
			return None;
		}
		let offsets = at + 1;
		let mut index = Self {
			count,
			off_size,
			offsets,
			base: offsets + (count + 1) * off_size - 1,
			end: 0,
		};
		index.end = index.base + index.offset(cff, count)?;
		(index.end <= cff.len()).then_some(index)
	}

	fn offset(&self, cff: &[u8], i: usize) -> Option<usize> {
		let bytes = cff.slice_at(self.offsets + i * self.off_size, self.off_size)?;
		Some(
			bytes
				.iter()
				.fold(0, |value, &byte| value << 8 | usize::from(byte)),
		)
	}

	fn get<'a>(&self, cff: &'a [u8], i: usize) -> Option<&'a [u8]> {
		if i >= self.count {
			return None;
		}
		let (start, end) = (self.offset(cff, i)?, self.offset(cff, i + 1)?);
		cff.get(self.base + start..self.base + end)
	}

	/// Subroutine numbers are stored minus a bias that depends on the count.
	fn bias(&self) -> i32 {
		match self.count {
			0..1240 => 107,
			1240..33900 => 1131,
			_ => 32768,
		}
	}
}

/// Calls `op(operator, operands)` for each DICT entry; two-byte operators
/// are `1200 + second byte`. Operators 22 to 24 are CFF2's.
fn dict(data: &[u8], mut op: impl FnMut(u16, &[f64])) -> Option<()> {
	let (mut stack, mut len, mut at) = ([0f64; MAX_STACK], 0, 0);
	while let Some(&b0) = data.get(at) {
		at += 1;
		let value = match b0 {
			0..=27 => {
				let key = if b0 == 12 {
					at += 1;
					1200 + u16::from(data.u8_at(at - 1)?)
				} else {
					u16::from(b0)
				};
				op(key, &stack[..len]);
				len = 0;
				continue;
			}
			28 => {
				at += 2;
				f64::from(data.i16_at(at - 2)?)
			}
			29 => {
				at += 4;
				f64::from(data.u32_at(at - 4)? as i32)
			}
			30 => {
				let (value, used) = real(&data[at..])?;
				at += used;
				value
			}
			32..=246 => f64::from(b0) - 139.0,
			247..=250 => {
				at += 1;
				f64::from(b0 - 247) * 256.0 + f64::from(data.u8_at(at - 1)?) + 108.0
			}
			251..=254 => {
				at += 1;
				-f64::from(b0 - 251) * 256.0 - f64::from(data.u8_at(at - 1)?) - 108.0
			}
			_ => return None,
		};
		if len < MAX_STACK {
			stack[len] = value;
			len += 1;
		}
	}
	Some(())
}

/// A DICT real number: nibbles of digits, point, exponent and sign, ended
/// by 0xf. Returns the value and the bytes used.
fn real(data: &[u8]) -> Option<(f64, usize)> {
	let mut text = String::new();
	for (i, &byte) in data.iter().enumerate() {
		for nibble in [byte >> 4, byte & 15] {
			match nibble {
				0..=9 => text.push(char::from(b'0' + nibble)),
				0xa => text.push('.'),
				0xb => text.push('E'),
				0xc => text.push_str("E-"),
				0xe => text.push('-'),
				0xf => return Some((text.parse().unwrap_or(0.0), i + 1)),
				_ => return None,
			}
		}
		if text.len() > 64 {
			return None;
		}
	}
	None
}

/// Local subroutines, and the variation data CFF2 charstrings blend with.
#[derive(Clone, Copy, Debug, Default)]
struct Local {
	subrs: Index,
	vsindex: usize,
}

/// The local subroutines of the Private DICT `(size, offset)`.
fn local(cff: &[u8], (size, offset): (usize, usize), wide: bool) -> Local {
	let (mut subrs, mut vsindex) = (None, 0);
	if let Some(private) = cff.slice_at(offset, size) {
		dict(private, |op, args| match (op, args) {
			(19, [.., at]) => subrs = Some(*at as usize),
			(22, [.., index]) => vsindex = *index as usize,
			_ => {}
		});
	}
	Local {
		subrs: subrs
			.and_then(|at| Index::parse(cff, offset.checked_add(at)?, wide))
			.unwrap_or_default(),
		vsindex,
	}
}

/// The region count of each ItemVariationData in a CFF2 variation store:
/// how many deltas follow each blended value.
fn regions(cff: &[u8], at: usize) -> Option<Vec<usize>> {
	// The store follows a 16-bit length.
	let store = at + 2;
	let count = usize::from(cff.u16_at(store + 6)?);
	(0..count.min(256))
		.map(|i| {
			let data = store + cff.u32_at(store + 8 + 4 * i)? as usize;
			cff.u16_at(data + 4).map(usize::from)
		})
		.collect()
}

/// The Private DICT `(size, offset)` a Top or Font DICT points to.
fn private(top: &[u8]) -> Option<(usize, usize)> {
	let mut private = None;
	dict(top, |op, args| {
		if let (18, [.., size, offset]) = (op, args) {
			private = Some((*size as usize, *offset as usize));
		}
	})?;
	private
}

#[derive(Debug)]
pub(crate) struct Cff {
	table: Table,
	charstrings: Index,
	global: Index,
	/// One set of local subroutines, or one per font in a CID font.
	local: Vec<Local>,
	/// Offset of the FDSelect table in CID fonts.
	fd_select: Option<usize>,
	/// CFF2: no advance widths in charstrings, and blended values.
	pub(super) version2: bool,
	regions: Vec<usize>,
}

impl Cff {
	pub(crate) fn parse(data: &[u8], table: Table, version2: bool) -> Result<Self, Error> {
		let cff = table.bytes(data);
		let parse = || -> Option<Self> {
			let header = usize::from(cff.u8_at(2)?);
			let (top, global) = if version2 {
				let top = cff.slice_at(header, usize::from(cff.u16_at(3)?))?;
				(top, Index::parse(cff, header + top.len(), true)?)
			} else {
				let names = Index::parse(cff, header, false)?;
				let tops = Index::parse(cff, names.end, false)?;
				let strings = Index::parse(cff, tops.end, false)?;
				(tops.get(cff, 0)?, Index::parse(cff, strings.end, false)?)
			};
			let (mut charstrings, mut fd_array, mut fd_select, mut store) = (None, None, None, None);
			dict(top, |op, args| match (op, args) {
				(17, [.., at]) => charstrings = Some(*at as usize),
				(24, [.., at]) => store = Some(*at as usize),
				(1236, [.., at]) => fd_array = Some(*at as usize),
				(1237, [.., at]) => fd_select = Some(*at as usize),
				_ => {}
			})?;
			let local = match fd_array {
				Some(at) => {
					let fonts = Index::parse(cff, at, version2)?;
					(0..fonts.count.min(256))
						.map(|i| {
							let font = fonts.get(cff, i).unwrap_or_default();
							private(font).map_or_else(Local::default, |p| local(cff, p, version2))
						})
						.collect()
				}
				None => vec![private(top).map_or_else(Local::default, |p| local(cff, p, version2))],
			};
			Some(Self {
				table,
				charstrings: Index::parse(cff, charstrings?, version2)?,
				global,
				local,
				fd_select: fd_select.filter(|_| fd_array.is_some()),
				version2,
				regions: store.and_then(|at| regions(cff, at)).unwrap_or_default(),
			})
		};
		parse().ok_or(MALFORMED)
	}

	/// The font a CID-keyed glyph belongs to.
	fn font_of(&self, cff: &[u8], glyph: u16) -> Option<usize> {
		let Some(at) = self.fd_select else {
			return Some(0);
		};
		let glyph = usize::from(glyph);
		match cff.u8_at(at)? {
			0 => cff.u8_at(at + 1 + glyph).map(usize::from),
			3 => {
				let ranges = usize::from(cff.u16_at(at + 1)?);
				(0..ranges).find_map(|i| {
					let range = at + 3 + 3 * i;
					let next = usize::from(cff.u16_at(range + 3)?);
					let first = usize::from(cff.u16_at(range)?);
					(first <= glyph && glyph < next).then(|| cff.u8_at(range + 2).map(usize::from))?
				})
			}
			4 => {
				let ranges = cff.u32_at(at + 1)? as usize;
				(0..ranges.min(65536)).find_map(|i| {
					let range = at + 5 + 6 * i;
					let next = cff.u32_at(range + 6)? as usize;
					let first = cff.u32_at(range)? as usize;
					(first <= glyph && glyph < next).then(|| cff.u16_at(range + 4).map(usize::from))?
				})
			}
			_ => None,
		}
	}

	pub(crate) fn outline(&self, data: &[u8], glyph: u16, sink: &mut dyn Sink) -> Option<Bounds> {
		let cff = self.table.bytes(data);
		let code = self.charstrings.get(cff, usize::from(glyph))?;
		let local = *self.local.get(self.font_of(cff, glyph)?)?;
		let mut charstring = Charstring {
			cff,
			global: self.global,
			local: local.subrs,
			regions: &self.regions,
			vsindex: local.vsindex,
			stack: [0.0; MAX_STACK],
			len: 0,
			x: 0.0,
			y: 0.0,
			stems: 0,
			width_seen: self.version2,
			open: false,
			operators: 0,
			sink,
			bounds: None,
		};
		charstring.run(code, 0)?;
		charstring.close();
		charstring.bounds
	}
}

/// The state of one Type 2 charstring being interpreted.
struct Charstring<'a> {
	cff: &'a [u8],
	global: Index,
	local: Index,
	/// Deltas per blended value, by variation data index.
	regions: &'a [usize],
	vsindex: usize,
	stack: [f32; MAX_STACK],
	len: usize,
	x: f32,
	y: f32,
	stems: usize,
	/// The advance width may lead the first stack-clearing operator.
	width_seen: bool,
	open: bool,
	operators: usize,
	sink: &'a mut dyn Sink,
	bounds: Option<Bounds>,
}

impl Charstring<'_> {
	/// Takes the operands, minus the advance width if `has_width` and this
	/// is the first stack-clearing operator.
	fn args(&mut self, has_width: bool) -> Vec<f32> {
		let skip = usize::from(has_width && !self.width_seen);
		self.width_seen = true;
		let args = self.stack[skip.min(self.len)..self.len].to_vec();
		self.len = 0;
		args
	}

	fn push(&mut self, value: f32) -> Option<()> {
		*self.stack.get_mut(self.len)? = value;
		self.len += 1;
		Some(())
	}

	fn pop(&mut self) -> Option<f32> {
		self.len = self.len.checked_sub(1)?;
		Some(self.stack[self.len])
	}

	fn close(&mut self) {
		if self.open {
			self.sink.close();
			self.open = false;
		}
	}

	fn move_by(&mut self, dx: f32, dy: f32) {
		self.close();
		(self.x, self.y) = (self.x + dx, self.y + dy);
		Bounds::include(&mut self.bounds, self.x, self.y);
		self.sink.move_to(self.x, self.y);
		self.open = true;
	}

	fn line_by(&mut self, dx: f32, dy: f32) {
		(self.x, self.y) = (self.x + dx, self.y + dy);
		Bounds::include(&mut self.bounds, self.x, self.y);
		self.sink.line_to(self.x, self.y);
	}

	fn curve_by(&mut self, d: [f32; 6]) {
		let (x1, y1) = (self.x + d[0], self.y + d[1]);
		let (x2, y2) = (x1 + d[2], y1 + d[3]);
		(self.x, self.y) = (x2 + d[4], y2 + d[5]);
		for (x, y) in [(x1, y1), (x2, y2), (self.x, self.y)] {
			Bounds::include(&mut self.bounds, x, y);
		}
		self.sink.curve_to(x1, y1, x2, y2, self.x, self.y);
	}

	/// Runs `code`; `Some(true)` once `endchar` is reached.
	fn run(&mut self, code: &[u8], depth: usize) -> Option<bool> {
		let mut at = 0;
		while let Some(&op) = code.get(at) {
			at += 1;
			self.operators += 1;
			if self.operators > MAX_OPERATORS {
				return None;
			}
			match op {
				// Stem hints: only their count matters, to size hint masks.
				1 | 3 | 18 | 23 => {
					let odd = self.len % 2 == 1;
					self.stems += self.args(odd).len() / 2;
				}
				19 | 20 => {
					let odd = self.len % 2 == 1;
					self.stems += self.args(odd).len() / 2;
					at += self.stems.div_ceil(8);
				}
				21 => {
					let width = self.len > 2;
					let a = self.args(width);
					self.move_by(*a.first()?, *a.get(1)?);
				}
				22 => {
					let width = self.len > 1;
					let dx = *self.args(width).first()?;
					self.move_by(dx, 0.0);
				}
				4 => {
					let width = self.len > 1;
					let dy = *self.args(width).first()?;
					self.move_by(0.0, dy);
				}
				5 => {
					for pair in self.args(false).as_chunks::<2>().0 {
						self.line_by(pair[0], pair[1]);
					}
				}
				6 | 7 => {
					for (i, &d) in self.args(false).iter().enumerate() {
						if (i % 2 == 0) == (op == 6) {
							self.line_by(d, 0.0);
						} else {
							self.line_by(0.0, d);
						}
					}
				}
				8 => {
					for c in self.args(false).as_chunks::<6>().0 {
						self.curve_by(*c);
					}
				}
				24 => {
					let a = self.args(false);
					let (curves, line) = a.split_at(a.len().checked_sub(2)?);
					for c in curves.as_chunks::<6>().0 {
						self.curve_by(*c);
					}
					self.line_by(line[0], line[1]);
				}
				25 => {
					let a = self.args(false);
					let (lines, curve) = a.split_at(a.len().checked_sub(6)?);
					for pair in lines.as_chunks::<2>().0 {
						self.line_by(pair[0], pair[1]);
					}
					self.curve_by([curve[0], curve[1], curve[2], curve[3], curve[4], curve[5]]);
				}
				26 | 27 => {
					let a = self.args(false);
					let (mut extra, rest) = if a.len() % 2 == 1 {
						(a[0], &a[1..])
					} else {
						(0.0, &a[..])
					};
					for c in rest.as_chunks::<4>().0 {
						self.curve_by(if op == 26 {
							[extra, c[0], c[1], c[2], 0.0, c[3]]
						} else {
							[c[0], extra, c[1], c[2], c[3], 0.0]
						});
						extra = 0.0;
					}
				}
				30 | 31 => {
					let a = self.args(false);
					let mut vertical = op == 30;
					let mut rest = &a[..];
					while rest.len() >= 4 {
						let last = if rest.len() == 5 { rest[4] } else { 0.0 };
						let c = rest;
						self.curve_by(if vertical {
							[0.0, c[0], c[1], c[2], c[3], last]
						} else {
							[c[0], 0.0, c[1], c[2], last, c[3]]
						});
						rest = &rest[4.min(rest.len())..];
						if rest.len() == 1 {
							break;
						}
						vertical = !vertical;
					}
				}
				10 | 29 => {
					let subrs = if op == 10 { self.local } else { self.global };
					let number = self.pop()? as i32 + subrs.bias();
					let subr = subrs.get(self.cff, usize::try_from(number).ok()?)?;
					if depth >= MAX_CALL_DEPTH {
						return None;
					}
					if self.run(subr, depth + 1)? {
						return Some(true);
					}
				}
				11 => return Some(false),
				// CFF2 variations: choose the variation data, or blend `n`
				// values at the default instance by dropping their deltas.
				15 => self.vsindex = self.pop()? as usize,
				16 => {
					let n = self.pop()? as usize;
					let deltas = n.checked_mul(*self.regions.get(self.vsindex)?)?;
					self.len = self.len.checked_sub(deltas)?;
				}
				14 => {
					let width = self.len % 2 == 1;
					self.args(width);
					self.close();
					return Some(true);
				}
				12 => {
					let escape = *code.get(at)?;
					at += 1;
					let a = self.args(false);
					self.flex(escape, &a)?;
				}
				28 => {
					at += 2;
					self.push(f32::from(code.i16_at(at - 2)?))?;
				}
				32..=246 => self.push(f32::from(op) - 139.0)?,
				247..=250 => {
					at += 1;
					self.push(f32::from(op - 247) * 256.0 + f32::from(*code.get(at - 1)?) + 108.0)?;
				}
				251..=254 => {
					at += 1;
					self.push(-f32::from(op - 251) * 256.0 - f32::from(*code.get(at - 1)?) - 108.0)?;
				}
				255 => {
					at += 4;
					self.push(code.u32_at(at - 4)? as i32 as f32 / 65536.0)?;
				}
				_ => self.len = 0,
			}
		}
		Some(false)
	}

	/// The flex operators: two curves meant to flatten at small sizes, drawn
	/// here as curves. Other escaped operators are ignored.
	fn flex(&mut self, escape: u8, a: &[f32]) -> Option<()> {
		match escape {
			35 if a.len() >= 12 => {
				self.curve_by([a[0], a[1], a[2], a[3], a[4], a[5]]);
				self.curve_by([a[6], a[7], a[8], a[9], a[10], a[11]]);
			}
			34 if a.len() >= 7 => {
				self.curve_by([a[0], 0.0, a[1], a[2], a[3], 0.0]);
				self.curve_by([a[4], 0.0, a[5], -a[2], a[6], 0.0]);
			}
			36 if a.len() >= 9 => {
				self.curve_by([a[0], a[1], a[2], a[3], a[4], 0.0]);
				let dy = -(a[1] + a[3] + a[7]);
				self.curve_by([a[5], 0.0, a[6], a[7], a[8], dy]);
			}
			37 if a.len() >= 11 => {
				let (start_x, start_y) = (self.x, self.y);
				self.curve_by([a[0], a[1], a[2], a[3], a[4], a[5]]);
				let (dx, dy) = (
					a[0] + a[2] + a[4] + a[6] + a[8],
					a[1] + a[3] + a[5] + a[7] + a[9],
				);
				// The last coordinate is x or y, whichever moved further.
				let (x6, y6) = if dx.abs() > dy.abs() {
					(a[10], start_y - (self.y + a[7] + a[9]))
				} else {
					(start_x - (self.x + a[6] + a[8]), a[10])
				};
				self.curve_by([a[6], a[7], a[8], a[9], x6, y6]);
			}
			_ => {}
		}
		Some(())
	}
}
