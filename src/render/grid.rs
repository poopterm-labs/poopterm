//! Draws the visible part of a terminal grid (cells, cursor, highlights, images).
use super::{
	glyphs::FontSet,
	images::draw_placement,
	painter::{Painter, glyph_top},
};
use crate::{
	graphics::{Placement, Rect},
	term::{Cell, CursorShape, Frame, NO_EXTRA, attr},
	ui::selection::CellPoint,
};

/// Everything needed to draw one terminal viewport.
pub(crate) struct GridView<'a> {
	pub frame: &'a Frame,
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
	/// Unfocused windows draw a hollow block cursor.
	pub focused: bool,
}

fn contains(range: Option<(CellPoint, CellPoint)>, point: CellPoint) -> bool {
	range.is_some_and(|(start, end)| point >= start && point <= end)
}

/// Images with a lower z-index are drawn under non-default cell backgrounds
/// too, not only under text (kitty graphics protocol).
const UNDER_BACKGROUNDS: i32 = i32::MIN / 2;

/// What part of a cell to draw: images can sit between backgrounds and text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
	All,
	Backgrounds,
	Text,
}

/// Draws the rows of `view` for which `rows` is true (all rows if `None`).
/// Glyphs and images are clipped to their row, so each row can be redrawn
/// alone.
pub(crate) fn draw_grid(
	p: &mut Painter,
	fonts: &mut FontSet,
	view: &GridView,
	rows: Option<&[bool]>,
) {
	let frame = view.frame;
	let mut images: Vec<&Placement> = frame.images.iter().collect();
	// Equal z: the lower image id is lower; Sixel images count as id 0.
	images.sort_by_key(|image| (image.z, image.kitty.map_or(0, |id| id.image)));
	let wanted = |row: usize| rows.is_none_or(|rows| rows[row]);
	for row in (0..frame.rows).filter(|&row| wanted(row)) {
		if images.is_empty() {
			draw_row(p, fonts, view, row, Layer::All);
			continue;
		}
		let clip = row_clip(p, view, row);
		draw_images(p, view, &images, clip, |z| z < UNDER_BACKGROUNDS);
		draw_row(p, fonts, view, row, Layer::Backgrounds);
		draw_images(p, view, &images, clip, |z| {
			(UNDER_BACKGROUNDS..0).contains(&z)
		});
		draw_row(p, fonts, view, row, Layer::Text);
		draw_images(p, view, &images, clip, |z| z >= 0);
	}
}

fn draw_row(p: &mut Painter, fonts: &mut FontSet, view: &GridView, row: usize, layer: Layer) {
	let frame = view.frame;
	let absolute_row = frame.first_row + row;
	let cells = frame.row(row);
	// Right to left, so a wide glyph is drawn after the cell it spills into.
	for (col, cell) in cells.iter().enumerate().rev() {
		let point = CellPoint {
			row: absolute_row,
			col,
		};
		let is_cursor = frame.cursor == Some((col, row));
		let solid_block = is_cursor && view.focused && frame.cursor_shape == CursorShape::Block;
		let (fg, bg) = if contains(view.selection, point) {
			view.selection_colors
		} else if contains(view.search_match, point) {
			view.match_colors
		} else if solid_block {
			(cell.bg, cell.fg)
		} else {
			(cell.fg, cell.bg)
		};
		draw_cell(p, fonts, view, cell, (col, row), (fg, bg), layer);
		if is_cursor && !solid_block && layer != Layer::Backgrounds {
			draw_cursor(p, view, cell, (col, row));
		}
	}
}

/// The pixels of grid row `row` inside the surface.
fn row_clip(p: &Painter, view: &GridView, row: usize) -> Rect {
	let (cw, ch) = view.cell;
	let (left, top) = (view.origin.0, view.origin.1 + row * ch);
	Rect {
		x: left,
		y: top,
		width: (view.frame.cols * cw).min(p.width.saturating_sub(left)),
		height: ch.min(p.height.saturating_sub(top)),
	}
}

fn draw_cell(
	p: &mut Painter,
	fonts: &mut FontSet,
	view: &GridView,
	cell: &Cell,
	(col, row): (usize, usize),
	(fg, bg): (u32, u32),
	layer: Layer,
) {
	let (cw, ch) = view.cell;
	let line = view.scale.max(1);
	let x0 = view.origin.0 + col * cw;
	let y0 = view.origin.1 + row * ch;
	if bg != view.default_bg && layer != Layer::Text {
		p.rect(x0, y0, cw, ch, bg);
	}
	if layer == Layer::Backgrounds {
		return;
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
	if (cell.ch == ' ' && cell.extra == NO_EXTRA) || cell.width == 0 {
		return;
	}
	let span = cw * cell.width as usize;
	let glyph = fonts.glyph(cell.ch, view.px);
	let origin = (
		x0 as i32 + glyph.metrics.xmin,
		glyph_top(glyph, y0, view.ascent),
	);
	let italic = cell.attrs & attr::ITALIC != 0;
	let (top, bottom) = (y0 as i32, (y0 + ch) as i32);
	p.glyph(
		glyph,
		origin,
		(x0 as i32, top, (x0 + span) as i32, bottom),
		italic,
		fg,
	);
	if cell.extra == NO_EXTRA {
		return;
	}
	if view.frame.is_link(cell) {
		p.rect(
			x0,
			y0 + ch.saturating_sub(2 * view.scale),
			span,
			line,
			view.link_color,
		);
	}
	for &mark in view.frame.marks(cell) {
		if mark == '\u{200d}' || mark == '\u{fe0f}' {
			continue;
		}
		let glyph = fonts.glyph(mark, view.px);
		let x = x0 as i32 + (cw as i32 - glyph.metrics.width as i32) / 2;
		let origin = (x, glyph_top(glyph, y0, view.ascent));
		p.glyph(
			glyph,
			origin,
			(x0 as i32, top, (x0 + cw) as i32, bottom),
			false,
			fg,
		);
	}
}

/// Underline, bar, or (when unfocused) hollow block cursors, drawn over the cell.
fn draw_cursor(p: &mut Painter, view: &GridView, cell: &Cell, (col, row): (usize, usize)) {
	let (cw, ch) = view.cell;
	let x = view.origin.0 + col * cw;
	let y = view.origin.1 + row * ch;
	let thick = 2 * view.scale.max(1);
	let width = cw * usize::from(cell.width.max(1));
	match (view.frame.cursor_shape, view.focused) {
		(CursorShape::Bar, _) => p.rect(x, y, thick, ch, cell.fg),
		(CursorShape::Underline, _) => p.rect(x, y + ch.saturating_sub(thick), width, thick, cell.fg),
		(CursorShape::Block, _) => {
			let line = view.scale.max(1);
			p.rect(x, y, width, line, cell.fg);
			p.rect(x, y + ch.saturating_sub(line), width, line, cell.fg);
			p.rect(x, y, line, ch, cell.fg);
			p.rect(x + width.saturating_sub(line), y, line, ch, cell.fg);
		}
	}
}

/// Draws the images whose z-index is in `layer`, in order, inside `clip`.
fn draw_images(
	p: &mut Painter,
	view: &GridView,
	images: &[&Placement],
	clip: Rect,
	layer: impl Fn(i32) -> bool,
) {
	let frame = view.frame;
	let (cw, ch) = view.cell;
	let (left, top) = view.origin;
	for image in images.iter().filter(|image| layer(image.z)) {
		let at = (
			(left + image.col * cw) as isize,
			top as isize + (image.row + frame.scroll_offset as isize) * ch as isize,
		);
		draw_placement(p.pixels, p.width, clip, image, at, view.cell);
	}
}
