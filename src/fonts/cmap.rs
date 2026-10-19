//! Character to glyph mapping (`cmap`): choosing the best Unicode subtable
//! and looking characters up in formats 0, 4, 6 and 12.
use super::read::Bytes;

/// A Unicode subtable inside the font data: its offset and format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cmap {
	offset: usize,
	format: u16,
}

impl Cmap {
	/// The best supported Unicode subtable of the `cmap` table at `table`:
	/// full repertoire (format 12) before the Basic Multilingual Plane.
	pub(crate) fn parse(data: &[u8], table: usize) -> Option<Self> {
		let count = usize::from(data.u16_at(table + 2)?);
		let mut best: Option<(u8, Self)> = None;
		for record in 0..count.min(256) {
			let at = table + 4 + 8 * record;
			let (platform, encoding) = (data.u16_at(at)?, data.u16_at(at + 2)?);
			let rank = match (platform, encoding) {
				(3, 10) | (0, 4 | 6) => 0,
				(3, 1) | (0, 0..=3) => 1,
				(3, 0) => 2,
				_ => continue,
			};
			let offset = table + data.u32_at(at + 4)? as usize;
			let Some(format) = data.u16_at(offset) else {
				continue;
			};
			if matches!(format, 0 | 4 | 6 | 12) && best.is_none_or(|(old, _)| rank < old) {
				best = Some((rank, Self { offset, format }));
			}
		}
		best.map(|(_, cmap)| cmap)
	}

	/// The glyph for `ch`, 0 (`.notdef`) when unmapped or malformed.
	pub(crate) fn glyph(&self, data: &[u8], ch: u32) -> u16 {
		let table = &data[self.offset.min(data.len())..];
		match self.format {
			0 => (ch < 256)
				.then(|| table.u8_at(6 + ch as usize))
				.flatten()
				.map_or(0, u16::from),
			4 => format4_glyph(table, ch).unwrap_or(0),
			6 => format6_glyph(table, ch).unwrap_or(0),
			12 => format12_glyph(table, ch).unwrap_or(0),
			_ => 0,
		}
	}
}

/// The glyph a format 4 subtable maps `ch` to; `None` if malformed.
pub(crate) fn format4_glyph(table: &[u8], ch: u32) -> Option<u16> {
	let ch = u16::try_from(ch).ok()?;
	let segments = usize::from(table.u16_at(6)? / 2);
	let (ends, starts) = (14, 16 + 2 * segments);
	let (deltas, ranges) = (starts + 2 * segments, starts + 4 * segments);
	// endCode is sorted: binary search for the first segment ending at or
	// after `ch`.
	let (mut low, mut high) = (0, segments);
	while low < high {
		let mid = (low + high) / 2;
		if table.u16_at(ends + 2 * mid)? < ch {
			low = mid + 1;
		} else {
			high = mid;
		}
	}
	let segment = low;
	if segment == segments {
		return Some(0);
	}
	let start = table.u16_at(starts + 2 * segment)?;
	if ch < start {
		return Some(0);
	}
	let delta = table.u16_at(deltas + 2 * segment)?;
	let range = table.u16_at(ranges + 2 * segment)?;
	if range == 0 {
		return Some(ch.wrapping_add(delta));
	}
	let at = ranges + 2 * segment + usize::from(range) + 2 * usize::from(ch - start);
	Some(match table.u16_at(at)? {
		0 => 0,
		glyph => glyph.wrapping_add(delta),
	})
}

fn format6_glyph(table: &[u8], ch: u32) -> Option<u16> {
	let (first, count) = (u32::from(table.u16_at(6)?), u32::from(table.u16_at(8)?));
	let index = ch.checked_sub(first).filter(|&index| index < count)?;
	table.u16_at(10 + 2 * index as usize)
}

/// The glyph a format 12 subtable maps `ch` to; groups are sorted, so a
/// binary search finds the one containing `ch`.
pub(crate) fn format12_glyph(table: &[u8], ch: u32) -> Option<u16> {
	let groups = table.u32_at(12)? as usize;
	let groups = groups.min(table.len().saturating_sub(16) / 12);
	let (mut low, mut high) = (0, groups);
	while low < high {
		let mid = (low + high) / 2;
		let at = 16 + 12 * mid;
		let (first, last) = (table.u32_at(at)?, table.u32_at(at + 4)?);
		if ch < first {
			high = mid;
		} else if ch > last {
			low = mid + 1;
		} else {
			let glyph = table.u32_at(at + 8)?.wrapping_add(ch - first);
			return Some(u16::try_from(glyph).unwrap_or(0));
		}
	}
	Some(0)
}
