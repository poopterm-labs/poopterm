//! Draws the visible part of a terminal grid (cells, cursor, highlights, Sixel images).
use super::{
	glyphs::FontSet,
	painter::{Painter, glyph_top},
};
use crate::{
	sixel,
	term::{Cell, Term, attr},
	ui::selection::CellPoint,
};

/// Everything needed to draw one terminal viewport.
pub(crate) struct GridView<'a> {
	pub term: &'a Term,
	/// Pixel position of the top-left cell.
	pub origin: (usize, usize),
	/// Cell width and height in pixels.
	pub cell: (usize, usize),
	pub px: f32,
	pub ascent: i32,
	pub scale: usize,
	/// Cells with this background are skipped; the caller already cleared to it.
	pub default_bg: u32,
	pub link_color: u32,
	pub selection_colors: (u32, u32),
	pub match_colors: (u32, u32),
	pub selection: Option<(CellPoint, CellPoint)>,
	pub search_match: Option<(CellPoint, CellPoint)>,
}

fn contains(range: Option<(CellPoint, CellPoint)>, point: CellPoint) -> bool {
	range.is_some_and(|(start, end)| point >= start && point <= end)
}

pub(crate) fn draw_grid(p: &mut Painter, fonts: &mut FontSet, view: &GridView) {
	let term = view.term;
	let history_len = term.scrollback.len();
	let screen_start = history_len.saturating_sub(term.scroll_offset);
	for row in 0..term.rows {
		let absolute_row = screen_start + row;
		let Some(cells) = term.row_cells(absolute_row) else {
			continue;
		};
		// Right to left, so a wide glyph is drawn after the cell it spills into.
		for (col, cell) in cells.iter().enumerate().rev() {
			let point = CellPoint {
				row: absolute_row,
				col,
			};
			let is_cursor =
				term.cursor_visible && term.scroll_offset == 0 && row == term.cy && col == term.cx;
			let (fg, bg) = if contains(view.selection, point) {
				view.selection_colors
			} else if contains(view.search_match, point) {
				view.match_colors
			} else if is_cursor {
				(cell.bg, cell.fg)
			} else {
				(cell.fg, cell.bg)
			};
			draw_cell(p, fonts, view, cell, (col, row), fg, bg);
		}
	}
	draw_images(p, view);
}

fn draw_cell(
	p: &mut Painter,
	fonts: &mut FontSet,
	view: &GridView,
	cell: &Cell,
	(col, row): (usize, usize),
	fg: u32,
	bg: u32,
) {
	let (cw, ch) = view.cell;
	let line = view.scale.max(1);
	let x0 = view.origin.0 + col * cw;
	let y0 = view.origin.1 + row * ch;
	if bg != view.default_bg {
		p.rect(x0, y0, cw, ch, bg);
	}
	if cell.attrs & attr::UNDERLINE != 0 {
		p.rect(x0, y0 + ch.saturating_sub(2 * view.scale), cw, line, fg);
	}
	if cell.attrs & attr::DOUBLE_UNDERLINE != 0 {
		p.rect(x0, y0 + ch.saturating_sub(4 * view.scale), cw, line, fg);
	}
	if cell.attrs & attr::OVERLINE != 0 {
		p.rect(x0, y0 + view.scale, cw, line, fg);
	}
	if cell.attrs & attr::STRIKE != 0 {
		p.rect(x0, y0 + ch / 2, cw, line, fg);
	}
	if (cell.ch == ' ' && cell.combining_len == 0) || cell.width == 0 {
		return;
	}
	let span = cw * cell.width as usize;
	let glyph = fonts.glyph(cell.ch, view.px);
	let origin = (
		x0 as i32 + glyph.metrics.xmin,
		glyph_top(glyph, y0, view.ascent),
	);
	let italic = cell.attrs & attr::ITALIC != 0;
	p.glyph(glyph, origin, (x0 as i32, (x0 + span) as i32), italic, fg);
	if cell.hyperlink != 0 {
		p.rect(
			x0,
			y0 + ch.saturating_sub(2 * view.scale),
			span,
			line,
			view.link_color,
		);
	}
	for &mark in &cell.combining[..cell.combining_len as usize] {
		if mark == '\u{200d}' || mark == '\u{fe0f}' {
			continue;
		}
		let glyph = fonts.glyph(mark, view.px);
		let x = x0 as i32 + (cw as i32 - glyph.metrics.width as i32) / 2;
		let origin = (x, glyph_top(glyph, y0, view.ascent));
		p.glyph(glyph, origin, (x0 as i32, (x0 + cw) as i32), false, fg);
	}
}

fn draw_images(p: &mut Painter, view: &GridView) {
	let term = view.term;
	let (cw, ch) = view.cell;
	let (left, top) = view.origin;
	let bottom = (top + term.rows * ch).min(p.height) as isize;
	for image in &term.images {
		let image_x = left + image.col * cw;
		if image_x >= p.width {
			continue;
		}
		let image_y = top as isize + (image.row + term.scroll_offset as isize) * ch as isize;
		let draw_width = image.bitmap.width.min(p.width - image_x);
		for sy in 0..image.bitmap.height {
			let y = image_y + sy as isize;
			if y < top as isize || y >= bottom {
				continue;
			}
			let src = &image.bitmap.pixels[sy * image.bitmap.width..][..draw_width];
			let dst_start = y as usize * p.width + image_x;
			let dst = &mut p.pixels[dst_start..dst_start + draw_width];
			if image.bitmap.opaque {
				dst.copy_from_slice(src);
			} else {
				for (out, &pixel) in dst.iter_mut().zip(src) {
					if pixel != sixel::TRANSPARENT {
						*out = pixel;
					}
				}
			}
		}
	}
}
